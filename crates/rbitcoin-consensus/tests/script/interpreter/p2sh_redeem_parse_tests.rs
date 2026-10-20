use bitcoin::absolute::LockTime;
use bitcoin::script::Script;
use bitcoin::script::ScriptBuf;
use bitcoin::{Amount, Sequence, Transaction};
use bitcoin::{OutPoint, TxIn, TxOut, Witness};
use rbitcoin_consensus::script::interpreter::*;

#[test]
fn eval_sig_then_p2sh_style() {
    // scriptSig 00 01 51 → stack [[],[0x51]]
    let ss = [0x00u8, 0x01, 0x51];
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(1),
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }],
    };
    let sc = Script::from_bytes(&ss);
    let ctx = EvalContext::new(&tx, 0, Amount::from_sat(1), sc, SigVersion::Base);
    let mut stack = Vec::new();
    eval_script(sc, &mut stack, &ctx).expect("scriptSig");
    let redeem = stack.pop().unwrap();
    let rs = Script::from_bytes(&redeem);
    let ctx2 = EvalContext::new(&tx, 0, Amount::from_sat(1), rs, SigVersion::Base);
    let r = eval_script(rs, &mut stack, &ctx2);
    assert!(r.is_ok(), "{r:?}");
}
