use rbitcoin_node::inhibit::*;

#[test]
fn try_start_and_drop_is_safe() {
    // On hosts without systemd-inhibit this returns None; with it, Drop reaps.
    let g = SuspendInhibit::try_start("rbitcoin unit test");
    drop(g);
    let _ = SuspendInhibit::try_start("rbitcoin unit test 2");
}
