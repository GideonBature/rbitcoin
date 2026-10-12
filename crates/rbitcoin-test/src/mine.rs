//! Regtest block construction helpers for high-level consensus scenarios.

use bitcoin::absolute::LockTime;
use bitcoin::block::{Header, Version};
use bitcoin::hashes::Hash;
use bitcoin::script::ScriptBuf;
use bitcoin::transaction::Version as TxVersion;
use bitcoin::{
    Amount, Block, BlockHash, CompactTarget, OutPoint, Sequence, Transaction, TxIn, TxMerkleNode,
    TxOut, Witness,
};

/// BIP34 height encoding in coinbase scriptSig (Core `CScript << height`).
pub fn bip34_script(height: u32) -> ScriptBuf {
    ScriptBuf::from_bytes(rbitcoin_consensus::bip34_height_script(height))
}

pub fn coinbase_tx(height: u32, value: Amount) -> Transaction {
    rbitcoin_consensus::regtest_coinbase(height, value, ScriptBuf::from_bytes(vec![0x51]))
}

/// Simple P2WPKH-like anyone-can-spend spend of a previous OP_TRUE output is not needed;
/// for connect tests we spend OP_TRUE with empty scriptSig.
pub fn spend_anyone_can_spend(prev_txid: bitcoin::Txid, vout: u32, value: Amount) -> Transaction {
    Transaction {
        version: TxVersion::ONE,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: prev_txid,
                vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value,
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }],
    }
}

/// One-input, multi-output anyone-can-spend create (for multi-vout parent tests).
pub fn split_anyone_can_spend(
    prev_txid: bitcoin::Txid,
    vout: u32,
    values: &[Amount],
) -> Transaction {
    Transaction {
        version: TxVersion::ONE,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: prev_txid,
                vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: values
            .iter()
            .map(|value| TxOut {
                value: *value,
                script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
            })
            .collect(),
    }
}

/// Spend several OP_TRUE prevouts in one tx (one output).
pub fn spend_many_anyone_can_spend(prevs: &[(bitcoin::Txid, u32)], value: Amount) -> Transaction {
    Transaction {
        version: TxVersion::ONE,
        lock_time: LockTime::ZERO,
        input: prevs
            .iter()
            .map(|(txid, vout)| TxIn {
                previous_output: OutPoint {
                    txid: *txid,
                    vout: *vout,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            })
            .collect(),
        output: vec![TxOut {
            value,
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }],
    }
}

/// Mine a regtest block on top of `prev_hash` (or genesis prev null).
pub fn mine_regtest_block(
    prev_hash: BlockHash,
    time: u32,
    height: u32,
    extra_txs: Vec<Transaction>,
) -> Block {
    mine_regtest_block_at(
        prev_hash,
        time,
        height,
        CompactTarget::from_consensus(0x207f_ffff),
        extra_txs,
    )
}

/// Mine a regtest block at an explicit `nBits` and timestamp.
///
/// Always pays a fixed 50 BTC `OP_TRUE` coinbase and does not add a BIP141
/// commitment. Callers that want the halving subsidy or a witness commitment
/// use [`rbitcoin_consensus::mine_regtest_paying_at`].
pub fn mine_regtest_block_at(
    prev_hash: BlockHash,
    time: u32,
    height: u32,
    bits: CompactTarget,
    extra_txs: Vec<Transaction>,
) -> Block {
    let mut txdata = vec![coinbase_tx(height, Amount::from_sat(50_0000_0000))];
    txdata.extend(extra_txs);
    let mut block = Block {
        header: Header {
            version: Version::from_consensus(4),
            prev_blockhash: prev_hash,
            merkle_root: TxMerkleNode::from_byte_array([0u8; 32]),
            time,
            bits,
            nonce: 0,
        },
        txdata,
    };
    block.header.merkle_root = block
        .compute_merkle_root()
        .expect("non-empty block has merkle root");
    rbitcoin_consensus::grind_regtest_pow(&mut block.header);
    block
}

pub fn regtest_genesis() -> Block {
    bitcoin::blockdata::constants::genesis_block(bitcoin::Network::Regtest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Target;

    #[test]
    fn bip34_and_coinbase_height_zero_and_pow() {
        // height 0 pads scriptSig to min length 2.
        let cb0 = coinbase_tx(0, Amount::from_sat(50));
        assert!(cb0.is_coinbase());
        assert!(cb0.input[0].script_sig.len() >= 2);
        // bip34_script is a thin wrapper — hit for several heights.
        for h in [1u32, 16, 0x80, 0x8000, 100_000] {
            let s = bip34_script(h);
            assert!(!s.is_empty());
        }
        // Non-zero coinbase uses bip34 encoding path (not the height-0 pad branch).
        let cb1 = coinbase_tx(1, Amount::from_sat(50));
        assert!(cb1.input[0].script_sig.len() >= 2);
        let g = regtest_genesis();
        let b = mine_regtest_block(g.block_hash(), g.header.time + 1, 1, vec![]);
        assert_eq!(b.header.prev_blockhash, g.block_hash());
        assert!(!b.txdata.is_empty());
        // POW found (regtest trivial).
        let target = Target::from_compact(b.header.bits);
        assert!(b.header.validate_pow(target).is_ok());

        // Extra helpers: multi-vout + multi-in spends (fixture surface).
        let prev = b.txdata[0].compute_txid();
        let split = split_anyone_can_spend(prev, 0, &[Amount::from_sat(10), Amount::from_sat(10)]);
        assert_eq!(split.output.len(), 2);
        let many = spend_many_anyone_can_spend(&[(prev, 0), (prev, 0)], Amount::from_sat(1));
        assert_eq!(many.input.len(), 2);
        let single = spend_anyone_can_spend(prev, 0, Amount::from_sat(1));
        assert_eq!(single.input.len(), 1);
    }

    /// The test helper always pays 50 BTC and never adds a BIP141 commitment.
    /// Subsidy halving and witness commitment belong to `mine_regtest_paying_at`.
    #[test]
    fn mine_regtest_block_keeps_fixed_coinbase_and_skips_witness_commitment() {
        let g = regtest_genesis();
        let halved = mine_regtest_block(g.block_hash(), g.header.time + 1, 150, vec![]);
        assert_eq!(halved.txdata[0].output.len(), 1);
        assert_eq!(
            halved.txdata[0].output[0].value,
            Amount::from_sat(50_0000_0000)
        );

        let mut extra =
            spend_anyone_can_spend(halved.txdata[0].compute_txid(), 0, Amount::from_sat(1));
        extra.input[0].witness = Witness::from_slice(&[&[0x01]]);
        let with_witness = mine_regtest_block(g.block_hash(), g.header.time + 2, 1, vec![extra]);
        assert_eq!(with_witness.txdata.len(), 2);
        assert_eq!(with_witness.txdata[0].output.len(), 1);
        assert!(with_witness.txdata[0].input[0].witness.is_empty());
        let target = Target::from_compact(with_witness.header.bits);
        assert!(with_witness.header.validate_pow(target).is_ok());
    }
}
