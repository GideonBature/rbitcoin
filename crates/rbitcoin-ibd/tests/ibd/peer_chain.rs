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
