use rbitcoin_net::*;

#[test]
fn outbound_defaults() {
    assert_eq!(DEFAULT_IBD_TARGET_PEERS, 16);
}
