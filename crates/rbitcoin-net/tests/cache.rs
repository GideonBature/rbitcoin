use rbitcoin_net::cache::*;

#[test]
fn default_body_depth_is_compact_serve_window() {
    assert_eq!(DEFAULT_BODY_DEPTH, 16);
}
