use bitcoin::{Amount, Target, Witness};
use rbitcoin_test::mine::{
    bip34_script, coinbase_tx, mine_regtest_block, regtest_genesis, spend_anyone_can_spend,
    spend_many_anyone_can_spend, split_anyone_can_spend,
};

#[test]
fn bip34_and_coinbase_height_zero_and_pow() {
    // height 0 pads scriptSig to min length 2.
    let cb0 = coinbase_tx(0, Amount::from_sat(50));
    assert!(cb0.is_coinbase());
    assert!(cb0.input[0].script_sig.len() >= 2);
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
    let target = Target::from_compact(b.header.bits);
    assert!(b.header.validate_pow(target).is_ok());

    let prev = b.txdata[0].compute_txid();
    let split = split_anyone_can_spend(prev, 0, &[Amount::from_sat(10), Amount::from_sat(10)]);
    assert_eq!(split.output.len(), 2);
    let many = spend_many_anyone_can_spend(&[(prev, 0), (prev, 0)], Amount::from_sat(1));
    assert_eq!(many.input.len(), 2);
    let single = spend_anyone_can_spend(prev, 0, Amount::from_sat(1));
    assert_eq!(single.input.len(), 1);
}

/// The test helper always pays 50 BTC and never adds a BIP141 commitment.
/// Subsidy halving and witness commitment belong to `mine_regtest_paying`.
#[test]
fn mine_regtest_block_keeps_fixed_coinbase_and_skips_witness_commitment() {
    let g = regtest_genesis();
    let halved = mine_regtest_block(g.block_hash(), g.header.time + 1, 150, vec![]);
    assert_eq!(halved.txdata[0].output.len(), 1);
    assert_eq!(
        halved.txdata[0].output[0].value,
        Amount::from_sat(50_0000_0000)
    );

    let mut extra = spend_anyone_can_spend(halved.txdata[0].compute_txid(), 0, Amount::from_sat(1));
    extra.input[0].witness = Witness::from_slice(&[&[0x01]]);
    let with_witness = mine_regtest_block(g.block_hash(), g.header.time + 2, 1, vec![extra]);
    assert_eq!(with_witness.txdata.len(), 2);
    assert_eq!(with_witness.txdata[0].output.len(), 1);
    assert!(with_witness.txdata[0].input[0].witness.is_empty());
    let target = Target::from_compact(with_witness.header.bits);
    assert!(with_witness.header.validate_pow(target).is_ok());
}
