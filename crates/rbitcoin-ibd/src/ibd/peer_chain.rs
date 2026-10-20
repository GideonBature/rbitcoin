//! What one IBD peer has shown of its chain.
//!
//! Core asks a peer only for blocks on that peer's best known chain
//! (`pindexBestKnownBlock`). A peer whose announced headers leave the
//! download path is not asked for blocks above that point, and a peer that
//! answered `notfound` for a block is not asked for it again.

use bitcoin::block::Header;
use bitcoin::BlockHash;
use std::collections::{HashMap, HashSet};

/// `notfound` hashes kept per peer. Past the cap a peer may be asked again.
const LACKS_CAP: usize = 1024;

#[derive(Debug, Default)]
pub struct PeerChain {
    lacks: HashSet<BlockHash>,
    /// The peer's header at the first height above the confirmed tip that
    /// the download path did not hold when it was announced.
    forks_at: Option<(BlockHash, u32)>,
}

impl PeerChain {
    pub fn note_notfound(&mut self, hash: BlockHash) {
        if self.lacks.len() < LACKS_CAP {
            self.lacks.insert(hash);
        }
    }

    pub fn lacks(&self, hash: &BlockHash) -> bool {
        self.lacks.contains(hash)
    }

    /// A headers message or a block inv: the peer may have those blocks now.
    pub fn note_announcement(&mut self) {
        self.lacks.clear();
    }

    /// Compare announced `headers`, the first at `height`, with the path
    /// above the confirmed `tip`. The first header the path does not hold is
    /// where this peer may leave it. A later batch does not move that point
    /// up, and one that matches the path through it clears it.
    pub fn note_headers(
        &mut self,
        path: &HashMap<u32, BlockHash>,
        tip: u32,
        height: u32,
        headers: &[Header],
    ) {
        let mut matched_to = None;
        for (ht, hdr) in (height..).zip(headers) {
            if ht <= tip {
                continue;
            }
            let hash = hdr.block_hash();
            if path.get(&ht) == Some(&hash) {
                matched_to = Some(ht);
                continue;
            }
            if self.forks_at.is_none_or(|(_, fork)| ht <= fork) {
                self.forks_at = Some((hash, ht));
            }
            return;
        }
        if let (Some((_, fork)), Some(matched)) = (self.forks_at, matched_to) {
            if matched >= fork {
                self.forks_at = None;
            }
        }
    }

    /// The peer's chain holds a different block than the path at or below
    /// `height`, so it does not have the path block there.
    pub fn left_path_by(&self, path: &HashMap<u32, BlockHash>, height: u32) -> bool {
        self.forks_at.is_some_and(|(hash, fork)| {
            fork <= height && path.get(&fork).is_some_and(|p| *p != hash)
        })
    }
}
