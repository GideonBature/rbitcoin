use rbitcoin_ibd::ibd::PeerBookSession;
use rbitcoin_net::seeds::AddrMan;
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::Mutex;

fn sa(o: u8) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, o)), 18444)
}

#[test]
fn peer_book_session_injects_seeds_and_flushes_on_drop() {
    let shared = Arc::new(Mutex::new(AddrMan::new()));
    {
        let mut sess = PeerBookSession::new(
            Some(Arc::clone(&shared)),
            &[
                rbitcoin_net::NetAddr::Ip(sa(1)),
                rbitcoin_net::NetAddr::Ip(sa(2)),
            ],
        );
        assert!(sess
            .book()
            .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(1)))
            .is_some());
        assert!(sess
            .book()
            .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(2)))
            .is_some());
        // Mutate book via book_mut.
        sess.book_mut().add(sa(3));
        assert!(sess
            .book()
            .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(3)))
            .is_some());
        // Shared not yet flushed until drop/flush.
        assert!(shared
            .lock()
            .unwrap()
            .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(3)))
            .is_none());
        sess.flush();
        assert!(shared
            .lock()
            .unwrap()
            .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(3)))
            .is_some());
    }
    // Drop flushes again (idempotent).
    assert!(shared
        .lock()
        .unwrap()
        .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(1)))
        .is_some());

    // No shared book — seeds only, flush is a no-op.
    let sess2 = PeerBookSession::new(None, &[rbitcoin_net::NetAddr::Ip(sa(9))]);
    assert!(sess2
        .book()
        .entry_of(&rbitcoin_net::NetAddr::from_socket(sa(9)))
        .is_some());
    sess2.flush();
}
