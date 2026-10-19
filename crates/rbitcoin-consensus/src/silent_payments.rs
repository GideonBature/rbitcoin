//! BIP-352 Silent Payments **tweak server** (scan-side `A_tweak` only).
//!
//! libsecp256k1 has a `silentpayments` C module; rust-secp256k1 0.29 (bitcoin
//! 0.32) does not bind it. This module uses `PublicKey::combine_keys` +
//! `mul_tweak` plus script extract. Tweak *index* never stores a scan key.
//! Frigate `blockchain.silentpayments.subscribe` may pass a scan private key
//! in RAM for the session only.

use bitcoin::hashes::{hash160, sha256, Hash, HashEngine};
use bitcoin::key::{Parity, XOnlyPublicKey};
use bitcoin::script::{Instruction, Script};
use bitcoin::secp256k1::{All, PublicKey, Scalar, Secp256k1, SecretKey};
use bitcoin::{OutPoint, Transaction, TxOut, Witness};
use std::sync::OnceLock;

/// BIP341 NUMS internal key *H* (SHA256 of uncompressed *G* as x-only).
const NUMS_H: [u8; 32] = [
    0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
    0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0,
];

/// One Taproot output listed for Electrum tweaks `output_pubkeys`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaprootOut {
    pub vout: u32,
    pub xonly: [u8; 32],
    pub value: u64,
}

/// Server tweak plus Taproot outs for one eligible tx.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxTweak {
    /// `input_hash · ΣA` — 33-byte compressed.
    pub tweak: [u8; 33],
    pub output_pubkeys: Vec<TaprootOut>,
}

fn secp() -> &'static Secp256k1<All> {
    static S: OnceLock<Secp256k1<All>> = OnceLock::new();
    S.get_or_init(Secp256k1::new)
}

/// Compute the BIP-352 server tweak for `tx` given prevout scripts.
///
/// `prevouts.len()` must equal `tx.input.len()`. Returns `None` if the tx is
/// not silent-payment eligible (or the pubkey sum is infinity / invalid hash).
pub fn tweak_from_tx(tx: &Transaction, prevouts: &[TxOut]) -> Option<TxTweak> {
    if tx.input.len() != prevouts.len() {
        return None;
    }
    let output_pubkeys = taproot_outs(tx);
    if output_pubkeys.is_empty() {
        return None;
    }
    if prevouts
        .iter()
        .any(|p| p.script_pubkey.witness_version().map(|v| v.to_num()) > Some(1))
    {
        return None;
    }

    let mut keys = Vec::new();
    for (vin, prev) in tx.input.iter().zip(prevouts.iter()) {
        if let Some(pk) = extract_input_pubkey(
            prev.script_pubkey.as_script(),
            vin.script_sig.as_script(),
            &vin.witness,
        ) {
            keys.push(pk);
        }
    }
    if keys.is_empty() {
        return None;
    }
    let refs: Vec<&PublicKey> = keys.iter().collect();
    let a = PublicKey::combine_keys(&refs).ok()?;
    let tweak = input_hash_mul_a(tx, &a)?;
    Some(TxTweak {
        tweak,
        output_pubkeys,
    })
}

fn input_hash_mul_a(tx: &Transaction, a: &PublicKey) -> Option<[u8; 33]> {
    let mut smallest = outpoint_bytes(&tx.input[0].previous_output);
    for vin in tx.input.iter().skip(1) {
        smallest = smallest.min(outpoint_bytes(&vin.previous_output));
    }
    let ser_a = a.serialize();
    let mut msg = [0u8; 36 + 33];
    msg[..36].copy_from_slice(&smallest);
    msg[36..].copy_from_slice(&ser_a);
    let h = tagged_hash(b"BIP0352/Inputs", &msg);
    if h == [0u8; 32] {
        return None;
    }
    let scalar = Scalar::from_be_bytes(h).ok()?;
    let tweaked = a.mul_tweak(secp(), &scalar).ok()?;
    Some(tweaked.serialize())
}

/// BIP-352 `P_k = B_spend + t_k·G` for tweak `A` and integer `k`.
pub fn taproot_matches_scan(
    tweak_compressed: &[u8; 33],
    output_xonly: &[u8; 32],
    scan_sk: &SecretKey,
    spend_pk: &PublicKey,
    k: u32,
) -> bool {
    let Ok(a) = PublicKey::from_slice(tweak_compressed) else {
        return false;
    };
    let Ok(scan_scalar) = Scalar::from_be_bytes(scan_sk.secret_bytes()) else {
        return false;
    };
    let Ok(shared) = a.mul_tweak(secp(), &scan_scalar) else {
        return false;
    };
    let mut payload = [0u8; 37];
    payload[..33].copy_from_slice(&shared.serialize());
    payload[33..].copy_from_slice(&k.to_be_bytes());
    let t = tagged_hash(b"BIP0352/SharedSecret", &payload);
    let Ok(tk) = SecretKey::from_slice(&t) else {
        return false;
    };
    let tg = PublicKey::from_secret_key(secp(), &tk);
    let Ok(p) = PublicKey::combine_keys(&[spend_pk, &tg]) else {
        return false;
    };
    p.x_only_public_key().0.serialize() == *output_xonly
}

fn tagged_hash(tag: &[u8], payload: &[u8]) -> [u8; 32] {
    let tagh = sha256::Hash::hash(tag);
    let mut eng = sha256::Hash::engine();
    eng.input(tagh.as_ref());
    eng.input(tagh.as_ref());
    eng.input(payload);
    sha256::Hash::from_engine(eng).to_byte_array()
}

fn outpoint_bytes(op: &OutPoint) -> [u8; 36] {
    let mut b = [0u8; 36];
    b[..32].copy_from_slice(op.txid.as_byte_array());
    b[32..].copy_from_slice(&op.vout.to_le_bytes());
    b
}

fn taproot_outs(tx: &Transaction) -> Vec<TaprootOut> {
    let mut out = Vec::new();
    for (i, o) in tx.output.iter().enumerate() {
        if !o.script_pubkey.is_p2tr() {
            continue;
        }
        let mut xonly = [0u8; 32];
        xonly.copy_from_slice(&o.script_pubkey.as_bytes()[2..34]);
        out.push(TaprootOut {
            vout: i as u32,
            xonly,
            value: o.value.to_sat(),
        });
    }
    out
}

fn extract_input_pubkey(
    prev: &Script,
    script_sig: &Script,
    witness: &Witness,
) -> Option<PublicKey> {
    if prev.is_p2tr() {
        return extract_p2tr(prev.as_bytes(), witness);
    }
    if prev.is_p2wpkh() {
        return last_compressed_witness_pubkey(witness);
    }
    if prev.is_p2sh() {
        if !is_p2sh_p2wpkh_redeem(script_sig) {
            return None;
        }
        return last_compressed_witness_pubkey(witness);
    }
    if prev.is_p2pkh() {
        return p2pkh_script_pubkey(prev.as_bytes(), script_sig);
    }
    None
}

fn extract_p2tr(spk: &[u8], witness: &Witness) -> Option<PublicKey> {
    if nums_h_script_path(witness) {
        return None;
    }
    let xonly = XOnlyPublicKey::from_slice(&spk[2..34]).ok()?;
    // Even-Y lift (BIP340 / BIP-352 taproot inputs).
    Some(xonly.public_key(Parity::Even))
}

fn nums_h_script_path(witness: &Witness) -> bool {
    let items = witness_items_no_annex(witness);
    if items.len() < 2 {
        return false;
    }
    let cb = items[items.len() - 1];
    if cb.len() < 33 {
        return false;
    }
    cb[1..33] == NUMS_H
}

fn witness_items_no_annex(witness: &Witness) -> Vec<&[u8]> {
    let mut items: Vec<&[u8]> = witness.iter().collect();
    if items.len() >= 2 {
        if let Some(last) = items.last() {
            if !last.is_empty() && last[0] == 0x50 {
                items.pop();
            }
        }
    }
    items
}

fn last_compressed_witness_pubkey(witness: &Witness) -> Option<PublicKey> {
    let items = witness_items_no_annex(witness);
    let last = items.last()?;
    compressed_pubkey(last)
}

/// P2PKH scriptSigs are third-party malleable. Take the compressed key whose
/// HASH160 matches the prevout (BIP-352: parse even if the template is wrapped).
fn p2pkh_script_pubkey(spk: &[u8], script_sig: &Script) -> Option<PublicKey> {
    if spk.len() != 25 {
        return None;
    }
    let want = &spk[3..23];
    let mut last = None;
    for ins in script_sig.instructions() {
        let Ok(Instruction::PushBytes(b)) = ins else {
            continue;
        };
        if let Some(pk) = compressed_pubkey(b.as_bytes()) {
            let h = hash160::Hash::hash(&pk.serialize());
            if h.as_byte_array() == want {
                last = Some(pk);
            }
        }
    }
    last
}

fn compressed_pubkey(b: &[u8]) -> Option<PublicKey> {
    if b.len() == 33 && (b[0] == 0x02 || b[0] == 0x03) {
        PublicKey::from_slice(b).ok()
    } else {
        None
    }
}

fn is_p2sh_p2wpkh_redeem(script_sig: &Script) -> bool {
    let mut push: Option<Vec<u8>> = None;
    let mut n = 0usize;
    for ins in script_sig.instructions() {
        match ins {
            Ok(Instruction::PushBytes(b)) => {
                n += 1;
                push = Some(b.as_bytes().to_vec());
            }
            Ok(_) => return false,
            Err(_) => return false,
        }
    }
    if n != 1 {
        return false;
    }
    let Some(r) = push else {
        return false;
    };
    r.len() == 22 && r[0] == 0x00 && r[1] == 0x14
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::absolute::LockTime;
    use bitcoin::consensus::encode::deserialize;
    use bitcoin::hashes::Hash;
    use bitcoin::script::ScriptBuf;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{Amount, Sequence, TxIn};
    use serde_json::Value;
    use std::str::FromStr;
    #[test]
    fn classifier_edges() {
        let p2wpkh = {
            let mut v = vec![0x00, 0x14];
            v.extend(std::iter::repeat_n(0x11, 20));
            v
        };
        let ver = |spk: &[u8]| {
            Script::from_bytes(spk)
                .witness_version()
                .map(|v| v.to_num())
        };
        assert_eq!(ver(&p2wpkh), Some(0));
        assert_eq!(ver(&[0x00, 0x02, 0xaa, 0xbb]), Some(0));
        assert_eq!(ver(&[0x00, 0x14, 0x11, 0x11, 0x11]), None);
        assert_eq!(ver(&[0x00, 40, 0x22]), None);
        let mut v1 = vec![0x51, 0x20];
        v1.extend(std::iter::repeat_n(0x33, 32));
        assert_eq!(ver(&v1), Some(1));
        let mut v2 = vec![0x52, 0x20];
        v2.extend(std::iter::repeat_n(0x33, 32));
        assert_eq!(ver(&v2), Some(2));

        let mut short_cb = vec![0x01];
        short_cb.extend(std::iter::repeat_n(0x44, 31));
        let short = Witness::from_slice(&[vec![0x51], short_cb]);
        assert!(!nums_h_script_path(&short));
        let mut nums = vec![0xc0];
        nums.extend_from_slice(&NUMS_H);
        let path = Witness::from_slice(&[vec![0x51], nums]);
        assert!(nums_h_script_path(&path));

        let push = |payload: &[u8]| {
            let mut s = vec![payload.len() as u8];
            s.extend_from_slice(payload);
            ScriptBuf::from_bytes(s)
        };
        assert!(is_p2sh_p2wpkh_redeem(push(&p2wpkh).as_script()));
        let mut not_ver = p2wpkh.clone();
        not_ver[0] = 0x01;
        assert!(!is_p2sh_p2wpkh_redeem(push(&not_ver).as_script()));
        assert!(!is_p2sh_p2wpkh_redeem(push(&[0x51; 22]).as_script()));
        let mut not_prog = p2wpkh.clone();
        not_prog[1] = 0x15;
        assert!(!is_p2sh_p2wpkh_redeem(push(&not_prog).as_script()));

        let sk = SecretKey::from_slice(&[3u8; 32]).unwrap();
        let pk = PublicKey::from_secret_key(secp(), &sk);
        let mut tx = Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: bitcoin::Txid::from_byte_array([9u8; 32]),
                    vout: 0,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ZERO,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::ZERO,
                script_pubkey: ScriptBuf::from_bytes({
                    let mut s = vec![0x51, 0x20];
                    s.extend(std::iter::repeat_n(0xab, 32));
                    s
                }),
            }],
        };
        tx.input.push(TxIn {
            previous_output: OutPoint {
                txid: bitcoin::Txid::from_byte_array([0u8; 32]),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        });
        let hashed = input_hash_mul_a(&tx, &pk);
        tx.input.swap(0, 1);
        assert_eq!(input_hash_mul_a(&tx, &pk), hashed);
    }

    #[test]
    fn taproot_matches_scan_rejects_invalid_tweak() {
        let sk = SecretKey::from_slice(&[2u8; 32]).unwrap();
        let pk = PublicKey::from_secret_key(secp(), &sk);
        assert!(!taproot_matches_scan(&[0u8; 33], &[0u8; 32], &sk, &pk, 0));
        assert!(!taproot_matches_scan(&[2u8; 33], &[0u8; 32], &sk, &pk, 7));
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        rbitcoin_primitives::hex_decode(s).expect("hex")
    }

    fn decode_witness(s: &str) -> Witness {
        if s.is_empty() {
            return Witness::new();
        }
        let b = hex_bytes(s);
        deserialize::<Witness>(&b).unwrap_or_else(|_| Witness::from_slice(&[&b]))
    }

    fn tx_from_receiving(given: &Value) -> (Transaction, Vec<TxOut>) {
        let vin = given["vin"].as_array().expect("vin");
        let mut input = Vec::new();
        let mut prevouts = Vec::new();
        for v in vin {
            let txid = bitcoin::Txid::from_str(v["txid"].as_str().unwrap()).unwrap();
            let vout = v["vout"].as_u64().unwrap() as u32;
            let script_sig =
                ScriptBuf::from_bytes(hex_bytes(v["scriptSig"].as_str().unwrap_or("")));
            let wit = match &v["txinwitness"] {
                Value::String(s) => decode_witness(s),
                Value::Array(items) => {
                    let stacks: Vec<Vec<u8>> = items
                        .iter()
                        .filter_map(|x| x.as_str().map(hex_bytes))
                        .collect();
                    let refs: Vec<&[u8]> = stacks.iter().map(|s| s.as_slice()).collect();
                    Witness::from_slice(&refs)
                }
                _ => Witness::new(),
            };
            input.push(TxIn {
                previous_output: OutPoint { txid, vout },
                script_sig,
                sequence: Sequence::MAX,
                witness: wit,
            });
            let prev_hex = v["prevout"]["scriptPubKey"]["hex"].as_str().unwrap();
            prevouts.push(TxOut {
                value: Amount::from_sat(1),
                script_pubkey: ScriptBuf::from_bytes(hex_bytes(prev_hex)),
            });
        }
        let mut output = Vec::new();
        if let Some(outs) = given["outputs"].as_array() {
            for o in outs {
                let x = hex_bytes(o.as_str().unwrap());
                assert_eq!(x.len(), 32);
                let mut spk = vec![0x51, 0x20];
                spk.extend_from_slice(&x);
                output.push(TxOut {
                    value: Amount::from_sat(1),
                    script_pubkey: ScriptBuf::from_bytes(spk),
                });
            }
        }
        if output.is_empty() {
            output.push(TxOut {
                value: Amount::from_sat(1),
                script_pubkey: ScriptBuf::from_bytes({
                    let mut s = vec![0x51, 0x20];
                    s.extend_from_slice(&[0u8; 32]);
                    s
                }),
            });
        }
        (
            Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input,
                output,
            },
            prevouts,
        )
    }

    #[test]
    fn official_vectors_receiving_tweaks() {
        let raw = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/bip352_send_and_receive_test_vectors.json"
        ));
        let cases: Value = serde_json::from_str(raw).expect("vectors json");
        let mut n = 0u32;
        for case in cases.as_array().unwrap() {
            let comment = case["comment"].as_str().unwrap_or("");
            for rec in case["receiving"].as_array().unwrap() {
                let given = &rec["given"];
                let expected = &rec["expected"];
                let (tx, prev) = tx_from_receiving(given);
                let got = tweak_from_tx(&tx, &prev);
                match expected.get("tweak") {
                    Some(Value::String(exp)) => {
                        let t = got.unwrap_or_else(|| panic!("expected tweak for {comment}"));
                        assert_eq!(
                            rbitcoin_primitives::hex_encode(t.tweak),
                            exp.to_ascii_lowercase(),
                            "tweak mismatch: {comment}"
                        );
                        n += 1;
                    }
                    _ => {
                        assert!(
                            got.is_none(),
                            "expected skip for {comment}, got {:?}",
                            got.map(|g| rbitcoin_primitives::hex_encode(g.tweak))
                        );
                        n += 1;
                    }
                }
                let unlabeled = given["labels"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true);
                if unlabeled {
                    if let Some(scan_hex) = given["key_material"]["scan_priv_key"].as_str() {
                        if let Some(tweak_hex) = expected["tweak"].as_str() {
                            let scan = SecretKey::from_slice(&hex_bytes(scan_hex)).unwrap();
                            let spend_sk = SecretKey::from_slice(&hex_bytes(
                                given["key_material"]["spend_priv_key"].as_str().unwrap(),
                            ))
                            .unwrap();
                            let spend_pk = PublicKey::from_secret_key(secp(), &spend_sk);
                            let mut tw = [0u8; 33];
                            tw.copy_from_slice(&hex_bytes(tweak_hex));
                            if let Some(outs) = expected["outputs"].as_array() {
                                for o in outs {
                                    let Some(pk) = o["pub_key"].as_str() else {
                                        continue;
                                    };
                                    let mut x = [0u8; 32];
                                    x.copy_from_slice(&hex_bytes(pk));
                                    assert!(
                                        (0u32..8).any(|k| {
                                            taproot_matches_scan(&tw, &x, &scan, &spend_pk, k)
                                        }),
                                        "scan miss {comment}"
                                    );
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(n >= 28, "ran {n} receiving cases");
    }

    #[test]
    fn skip_witness_v2_input() {
        let tx = Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(1),
                script_pubkey: ScriptBuf::from_bytes({
                    let mut s = vec![0x51, 0x20];
                    s.extend_from_slice(&[1u8; 32]);
                    s
                }),
            }],
        };
        let mut v2 = vec![0x52, 0x14];
        v2.extend_from_slice(&[0u8; 20]);
        let prev = vec![TxOut {
            value: Amount::from_sat(1),
            script_pubkey: ScriptBuf::from_bytes(v2),
        }];
        assert!(tweak_from_tx(&tx, &prev).is_none());
    }
}
