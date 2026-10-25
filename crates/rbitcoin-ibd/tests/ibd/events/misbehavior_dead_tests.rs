use super::super::assign::dummy_slot;
use rbitcoin_ibd::ibd::state::IbdWorkState;
use rbitcoin_net::seeds::AddrMan;

use rbitcoin_ibd::ibd::events::apply_peer_dead;

#[test]
fn threshold_dead_cools_after_a_body_and_other_deaths_do_not() {
    let mut slot = dummy_slot(1);
    slot.first_data_ms = 42;
    let addr = slot.addr;
    let mut st = IbdWorkState::new(vec![slot], None, Some(0));
    let mut book = AddrMan::new();
    book.note_connected_addr(rbitcoin_net::NetAddr::from_socket(addr));
    apply_peer_dead(
        &mut st,
        &mut book,
        1,
        "peer misbehavior threshold".to_string(),
    );
    assert!(
        st.addr_cooldown
            .contains_key(&rbitcoin_net::NetAddr::from_socket(addr)),
        "misbehavior cools the dial after a block body"
    );
    assert!(book.flags(&addr).failed_last_connect());

    let mut other = dummy_slot(2);
    other.first_data_ms = 42;
    let other_addr = other.addr;
    let mut st = IbdWorkState::new(vec![other], None, Some(0));
    let mut book = AddrMan::new();
    book.note_connected_addr(rbitcoin_net::NetAddr::from_socket(other_addr));
    apply_peer_dead(&mut st, &mut book, 2, "bye".to_string());
    assert!(
        !st.addr_cooldown
            .contains_key(&rbitcoin_net::NetAddr::from_socket(other_addr)),
        "a body already counted keeps an ordinary death off the cooldown"
    );
    assert!(!book.flags(&other_addr).failed_last_connect());
}

#[test]
fn onion_death_and_explicit_disconnect_cool_the_actual_identity() {
    use rbitcoin_ibd::ibd::dial::{dial_blocked_addrs, disconnect_peer};
    use rbitcoin_net::NetAddr;
    use std::collections::HashSet;
    use std::time::Instant;

    for (first_data, reason, explicit, cooled) in [
        (0, "bye", false, true),
        (42, "bye", false, false),
        (42, "peer misbehavior threshold", false, true),
        (42, "", true, true),
    ] {
        let onion = NetAddr::Onion {
            pk: [7; 32],
            port: 8333,
        };
        let mut slot = dummy_slot(1);
        slot.addr = "0.0.0.0:8333".parse().unwrap();
        slot.net = onion;
        slot.first_data_ms = first_data;
        let mut st = IbdWorkState::new(vec![slot], None, Some(0));
        let mut book = AddrMan::new();
        book.set_onion_reachable(true);
        book.note_connected_addr(onion);
        if explicit {
            disconnect_peer(&mut st, 1);
            assert_eq!(st.addr_strikes.get(&onion), Some(&1));
        } else {
            apply_peer_dead(&mut st, &mut book, 1, reason.to_string());
            assert_eq!(book.connect_failed(onion), cooled);
        }
        assert!(!st.slots[0].alive);
        let expected = if cooled {
            HashSet::from([onion])
        } else {
            HashSet::new()
        };
        assert_eq!(
            dial_blocked_addrs(&[], &st.addr_cooldown, Instant::now()),
            expected
        );
    }
}
