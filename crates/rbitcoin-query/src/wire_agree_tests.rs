//! Store wire walk and consensus decode agree on every truncation.
#![cfg(test)]

use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::transaction::Version;
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};

fn legacy_1in() -> Transaction {
    Transaction {
        version: Version::ONE,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: bitcoin::Txid::from_byte_array([0x11; 32]),
                vout: 0,
            },
            script_sig: ScriptBuf::from_bytes(vec![0x51, 0x51]),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_000),
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }],
    }
}

fn p2wpkh_like() -> Transaction {
    let mut tx = legacy_1in();
    tx.input[0].script_sig = ScriptBuf::new();
    tx.input[0].witness = Witness::from_slice(&[vec![0x30; 71], vec![0x02; 33]]);
    tx
}

const FLAG_ZERO_TX: [u8; 10] = [2, 0, 0, 0, 0x00, 0x00, 7, 0, 0, 0];

/// Peer intake walks the wire and lookup decodes it. A payload one side
/// accepts and the other refuses either strands a queued body or drops an
/// honest one, so every truncation and single-byte rewrite must agree.
#[test]
fn block_wire_walk_agrees_with_decode() {
    use bitcoin::consensus::encode::serialize;
    let zero_in = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            value: Amount::from_sat(1),
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }],
    };
    // Segwit with one empty and one non-empty witness decodes; rewriting
    // the non-empty stack count to 0 hits the all-empty refusal.
    let mut mixed = p2wpkh_like();
    mixed.input.insert(0, legacy_1in().input[0].clone());
    mixed.input[1].witness = Witness::from_slice(&[[0x01]]);
    let mut raw = vec![0u8; 80];
    raw.push(5);
    for tx in [legacy_1in(), p2wpkh_like(), zero_in, mixed] {
        raw.extend_from_slice(&serialize(&tx));
    }
    raw.extend_from_slice(&FLAG_ZERO_TX);
    let agree = |p: &[u8]| {
        let decoded = rbitcoin_consensus::decode_block_precomputes(p, false)
            .map(|(b, _, _)| b.txdata.iter().map(|t| t.input.len() as u32).sum());
        assert_eq!(
            rbitcoin_store::block_wire_input_count(p),
            decoded,
            "payload {p:02x?}"
        );
        decoded
    };
    assert_eq!(agree(&raw), Some(4));
    for n in 0..raw.len() {
        agree(&raw[..n]);
    }
    for i in 80..raw.len() {
        for v in [0x00, 0x01, 0x02, 0xfd, 0xfe, 0xff] {
            let mut p = raw.clone();
            p[i] = v;
            agree(&p);
        }
    }
}
