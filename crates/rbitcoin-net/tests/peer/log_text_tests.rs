use super::non_version_before_handshake_log;
use super::unsupported_before_verack_log;

#[test]
fn peer_command_logs_have_no_raw_newline() {
    let line = non_version_before_handshake_log("inv\nX", 7);
    assert!(!line.contains('\n'), "{line:?}");
    assert!(!line.contains('\r'), "{line:?}");
    let line = unsupported_before_verack_log("ping\r\n", 3);
    assert!(!line.contains('\n'), "{line:?}");
    assert!(!line.contains('\r'), "{line:?}");
}
