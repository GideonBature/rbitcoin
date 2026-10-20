use rbitcoin_rpc::methods::chain::format_core_double;

#[test]
fn sixteen_significant_digits() {
    assert_eq!(format_core_double(1.0), "1");
    assert_eq!(format_core_double(0.0), "0");
    assert_eq!(format_core_double(132757073449487.52), "132757073449487.5");
}
