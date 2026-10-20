#[test]
fn node_rpc_path_is_root() {
    assert_eq!(rbitcoin_rpc::node_rpc_path(), "/");
}
