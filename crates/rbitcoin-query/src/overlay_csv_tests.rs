//! CSV activation overlay changes confirm, not only the height getter.
#![cfg(test)]

use bitcoin::absolute::LockTime;
use bitcoin::blockdata::constants;
use bitcoin::script::ScriptBuf;
use bitcoin::transaction::Version as TxVersion;
use bitcoin::{Amount, Network, OutPoint, Sequence, Transaction, TxIn, TxOut, Witness};
use rbitcoin_consensus::{mine_regtest_paying, ChainParams, Milestone};
use rbitcoin_primitives::Height;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{accept_and_connect_block, pad_empty_from, Query};

/// Overlay must change **confirm** BIP68, not just the getter.
#[test]
fn overlay_csv_102_is_lax_at_101_strict_at_102() {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rbtc-overlay-csv-{n}"));
    let q = Query::open_or_create_tiny(&dir).unwrap();
    let mut params = ChainParams::regtest();
    params.apply_test_activation_height("csv", 102).unwrap();
    let genesis = constants::genesis_block(Network::Regtest);
    accept_and_connect_block(&q, &params, Height::GENESIS, &genesis, Milestone::NONE).unwrap();
    let (tip, tip_time, cbs) = pad_empty_from(
        &q,
        &params,
        genesis.block_hash(),
        genesis.header.time,
        1,
        100,
        1,
    );
    let spk = ScriptBuf::from_bytes(vec![0x51]);
    let parent = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: cbs[0],
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_0000_0000 - 1_000),
            script_pubkey: spk.clone(),
        }],
    };
    let b101 = mine_regtest_paying(tip, tip_time + 600, 101, spk.clone(), vec![parent.clone()]);
    accept_and_connect_block(&q, &params, Height(101), &b101, Milestone::NONE)
        .expect("parent spend at 101");

    let child = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: parent.compute_txid(),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::from_consensus(10),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_0000_0000 - 2_000),
            script_pubkey: spk.clone(),
        }],
    };
    let b102 = mine_regtest_paying(
        b101.block_hash(),
        b101.header.time + 600,
        102,
        spk.clone(),
        vec![child],
    );
    let err =
        accept_and_connect_block(&q, &params, Height(102), &b102, Milestone::NONE).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("not final") || msg.contains("nonfinal") || msg.contains("BIP68"),
        "csv@102 must reject BIP68-locked spend at height 102: {msg}"
    );

    let dir2 = std::env::temp_dir().join(format!("rbtc-overlay-csv-lax-{n}"));
    let q2 = Query::open_or_create_tiny(&dir2).unwrap();
    let mut lax = ChainParams::regtest();
    lax.apply_test_activation_height("csv", 200).unwrap();
    accept_and_connect_block(&q2, &lax, Height::GENESIS, &genesis, Milestone::NONE).unwrap();
    let (tip2, time2, cbs2) = pad_empty_from(
        &q2,
        &lax,
        genesis.block_hash(),
        genesis.header.time,
        1,
        100,
        1,
    );
    let p_lax = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: cbs2[0],
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_0000_0000 - 1_000),
            script_pubkey: spk.clone(),
        }],
    };
    let b101l = mine_regtest_paying(tip2, time2 + 600, 101, spk.clone(), vec![p_lax.clone()]);
    accept_and_connect_block(&q2, &lax, Height(101), &b101l, Milestone::NONE).unwrap();
    let c_lax = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: p_lax.compute_txid(),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::from_consensus(10),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_0000_0000 - 2_000),
            script_pubkey: spk,
        }],
    };
    let b102l = mine_regtest_paying(
        b101l.block_hash(),
        b101l.header.time + 600,
        102,
        ScriptBuf::from_bytes(vec![0x51]),
        vec![c_lax],
    );
    accept_and_connect_block(&q2, &lax, Height(102), &b102l, Milestone::NONE)
        .expect("csv@200: BIP68 not consensus at height 102");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}
