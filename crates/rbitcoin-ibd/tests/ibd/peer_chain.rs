use bitcoin::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::{BlockHash, CompactTarget, TxMerkleNode};
use rbitcoin_ibd::ibd::peer_chain::PeerChain;
use std::collections::HashMap;

fn chain(n: u32, salt: u8) -> Vec<Header> {
    let mut prev = BlockHash::from_byte_array([0u8; 32]);
    (1..=n)
        .map(|i| {
            let mut h = Header {
                version: bitcoin::block::Version::from_consensus(4),
                prev_blockhash: prev,
                merkle_root: TxMerkleNode::from_byte_array([salt; 32]),
                time: 1_600_000_000 + i,
                bits: CompactTarget::from_consensus(0x207fffff),
                nonce: i,
            };
            rbitcoin_consensus::grind_regtest_pow(&mut h);
            prev = h.block_hash();
            h
        })
        .collect()
}

/// Two full `headers` replies and then a short one: the peer holds every
/// block of its newest reply, and none past that reply's top.
#[test]
fn an_announced_chain_keeps_the_newest_reply() {
    let headers = chain(4_010, 0x5c);
    let path: HashMap<u32, BlockHash> = (1..).zip(headers.iter().map(|h| h.block_hash())).collect();
    let mut peer = PeerChain::default();
    peer.note_headers(&path, |_| None, 0, 1, &headers[..2_000]);
    peer.note_headers(&path, |_| None, 0, 2_001, &headers[2_000..4_000]);
    peer.note_headers(&path, |_| None, 0, 4_001, &headers[4_000..]);
    assert!(
        !peer.left_path_by(&path, 4_005),
        "the newest reply is what the peer has"
    );
    assert!(!peer.left_path_by(&path, 3_000));
    let mut beyond = path.clone();
    beyond.insert(4_011, BlockHash::from_byte_array([7u8; 32]));
    assert!(
        peer.left_path_by(&beyond, 4_011),
        "a short reply is the end of that peer's chain"
    );
}

/// A full reply whose split height has since been confirmed and left the
/// path. This node's confirmed block there settles it: a different block
/// means the peer left this chain, the same block means this node took it.
#[test]
fn a_split_is_settled_by_the_confirmed_chain() {
    let ours = chain(2_001, 0x5c);
    let theirs = chain(2_000, 0x77);
    let mut path: HashMap<u32, BlockHash> =
        (1..).zip(ours.iter().map(|h| h.block_hash())).collect();
    let mut left = PeerChain::default();
    left.note_headers(&path, |_| None, 0, 1, &theirs);
    let mut taken = PeerChain::default();
    let mut before = path.clone();
    before.insert(1, theirs[0].block_hash());
    taken.note_headers(&before, |_| None, 0, 1, &ours[..2_000]);
    for h in 1..=2_000 {
        path.remove(&h);
    }
    let confirmed = |h: u32| (h <= 2_000).then(|| ours[h as usize - 1].block_hash());
    left.settle_split(&path, confirmed);
    taken.settle_split(&path, confirmed);
    assert!(
        left.left_path_by(&path, 2_001),
        "a peer that split where this node confirmed another block has left"
    );
    assert!(
        !taken.left_path_by(&path, 2_001),
        "a peer whose chain this node confirmed has not"
    );
}

/// A short reply that ends at or below the confirmed tip is all that peer
/// has, so it is not asked above the tip. That holds for a first reply and
/// for one that follows a short run this node has since confirmed.
#[test]
fn a_reply_at_or_below_the_tip_ends_the_chain_there() {
    let headers = chain(20, 0x5c);
    let above = |tip: u32| -> HashMap<u32, BlockHash> {
        (tip + 1..=20)
            .map(|h| (h, headers[h as usize - 1].block_hash()))
            .collect()
    };
    let confirmed = |h: u32| Some(headers[h as usize - 1].block_hash());
    let mut behind = PeerChain::default();
    behind.note_headers(&above(10), confirmed, 10, 5, &headers[4..8]);
    assert!(
        behind.left_path_by(&above(10), 11),
        "a first reply that ends below the tip"
    );
    let mut caught = PeerChain::default();
    caught.note_headers(&above(10), confirmed, 10, 11, &headers[10..15]);
    assert!(caught.left_path_by(&above(10), 16));
    caught.note_headers(&above(15), confirmed, 15, 13, &headers[12..15]);
    assert!(
        caught.left_path_by(&above(15), 16),
        "a reply that ends at the tip after the kept run was confirmed"
    );
}

/// A rewind keeps a split the peer itself showed at or below the fork. A
/// peer on a third chain stays off both chains when this node rewinds away
/// from its chain and back.
#[test]
fn a_third_chain_split_survives_two_rewinds() {
    let ours = chain(3, 0x5c);
    let won = chain(3, 0x77);
    let third = chain(3, 0x91);
    let path = |c: &[Header]| -> HashMap<u32, BlockHash> {
        (1..).zip(c.iter().map(|h| h.block_hash())).collect()
    };
    let confirmed = |h: u32| Some(ours[h as usize - 1].block_hash());
    let mut peer = PeerChain::default();
    peer.note_headers(&HashMap::new(), confirmed, 2, 1, &third);
    peer.note_rewind(&path(&won), 0, ours[0].block_hash());
    assert!(peer.left_path_by(&path(&won), 1));
    peer.note_rewind(&path(&ours), 0, won[0].block_hash());
    assert!(
        peer.left_path_by(&path(&ours), 1),
        "a peer on a third chain is not taken back onto this one"
    );
}

/// A peer that showed a stale sibling and then builds on this node's block
/// at or above that height has joined this chain: the split is gone.
#[test]
fn a_reply_on_this_chain_above_a_split_clears_it() {
    let ours = chain(3, 0x5c);
    let sibling = chain(1, 0x77);
    let path: HashMap<u32, BlockHash> = (1..).zip(ours.iter().map(|h| h.block_hash())).collect();
    let mut peer = PeerChain::default();
    peer.note_headers(&path, |_| None, 0, 1, &sibling);
    assert!(peer.left_path_by(&path, 2));
    peer.note_headers(&path, |_| None, 0, 2, &ours[1..]);
    assert!(!peer.left_path_by(&path, 2));
    assert!(!peer.left_path_by(&path, 3));
    assert!(
        peer.shows(&path, 3),
        "the peer is asked for this chain again"
    );

    // A reply on this node's block that leaves it at the next height: the
    // parent clears the old split and the new one is where it left.
    let mut moved = PeerChain::default();
    moved.note_headers(&path, |_| None, 0, 1, &sibling);
    let other = chain(2, 0x91);
    let off = Header {
        prev_blockhash: ours[0].block_hash(),
        ..other[1]
    };
    moved.note_headers(&path, |_| None, 0, 2, &[off]);
    assert!(
        !moved.left_path_by(&path, 1),
        "it has this node's block below"
    );
    assert!(moved.left_path_by(&path, 2));
}

/// A `notfound` keeps a peer off that block for one stall window, then the
/// peer may be asked again.
#[test]
fn a_notfound_lasts_one_stall_window() {
    let mut peer = PeerChain::default();
    let hash = BlockHash::from_byte_array([3u8; 32]);
    peer.note_notfound(hash, 1_000);
    assert!(peer.lacks(&hash, 1_000 + 29_999));
    assert!(
        !peer.lacks(&hash, 1_000 + 30_000),
        "past the stall window the peer is asked again"
    );
}
