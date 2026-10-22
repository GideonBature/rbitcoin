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
        st.addr_cooldown.contains_key(&addr),
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
        !st.addr_cooldown.contains_key(&other_addr),
        "a body already counted keeps an ordinary death off the cooldown"
    );
    assert!(!book.flags(&other_addr).failed_last_connect());
}
