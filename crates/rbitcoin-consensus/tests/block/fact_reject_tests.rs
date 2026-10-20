use rbitcoin_consensus::block::{reject_bip30_unspent_prior, reject_one_create};
use rbitcoin_consensus::error::ConsensusError;

#[test]
fn bip30_and_immature_coinbase_reject_from_facts() {
    assert!(reject_bip30_unspent_prior(2, 2, true).is_ok());
    let bip30 = reject_bip30_unspent_prior(5, 1, true).unwrap_err();
    assert!(
        matches!(bip30, ConsensusError::BadTx(s) if s.contains("BIP30")),
        "{bip30:?}"
    );
    assert!(reject_bip30_unspent_prior(5, 1, false).is_ok());

    let missing = reject_one_create(10, 100, None, false).unwrap_err();
    assert!(
        matches!(missing, ConsensusError::BadTx(s) if s.contains("missingorspent")),
        "{missing:?}"
    );
    let forward = reject_one_create(10, 100, Some(11), false).unwrap_err();
    assert!(
        matches!(forward, ConsensusError::MissingPrevout),
        "{forward:?}"
    );
    let immature = reject_one_create(10, 100, Some(1), true).unwrap_err();
    assert!(
        matches!(immature, ConsensusError::BadTx(s) if s.contains("immature")),
        "{immature:?}"
    );
    assert!(reject_one_create(101, 100, Some(1), true).is_ok());
}
