use rbitcoin_electrum::server::*;
use serde_json::json;

#[test]
fn silent_payment_log_drops_the_scan_secret() {
    let secret = "ab".repeat(32);
    let line = redact_sp_params(&json!([secret, "02ff", 1]));
    assert!(!line.contains(&secret), "{line}");
    assert!(line.contains("<redacted>"), "{line}");
}
