use rbitcoin_sv2::session::{constraints_rebuild_at, Templates};
use rbitcoin_sv2::CONSTRAINTS_COOLDOWN;
use std::time::Duration;
use tokio::time::Instant;

#[test]
fn constraints_rebuild_at_uses_the_cooldown() {
    let built_at = Instant::now();
    let soon = built_at + CONSTRAINTS_COOLDOWN - Duration::from_millis(1);
    assert_eq!(
        constraints_rebuild_at(built_at, soon),
        Some(built_at + CONSTRAINTS_COOLDOWN)
    );
    let due = built_at + CONSTRAINTS_COOLDOWN;
    assert_eq!(constraints_rebuild_at(built_at, due), None);
}

#[test]
fn tip_event_rebuilds_a_template_built_before_it() {
    let (old, new) = ([1; 32], [2; 32]);
    let mut t = Templates::default();
    assert!(t.on_tip(old), "first tip");
    t.current_prev = Some(old);
    assert!(!t.on_tip(old), "repeat event with no build since");
    // A build between the store tip move and its event (constraints, or
    // the previous event's rebuild) already sits on the new prev hash.
    t.current_prev = Some(new);
    t.built_since_tip = true;
    assert!(t.on_tip(new), "template may hold the block's confirmed txs");
    assert!(!t.on_tip(new), "no build since the event");
    // A failed reorg disconnects `new` without an event, then its rollback
    // reconnects `new`; a build in that window precedes the second event.
    t.built_since_tip = true;
    assert!(t.on_tip(new), "reconnect of the same hash after a build");
}
