//! What one IBD peer has shown of its chain.
//!
//! Core asks a peer only for blocks on that peer's best known chain
//! (`pindexBestKnownBlock`). A peer whose announced headers leave the
//! download path is not asked for blocks above that point, and a peer that
//! answered `notfound` for a block is not asked for it again.

use bitcoin::block::Header;
use bitcoin::BlockHash;
use std::collections::{HashMap, HashSet, VecDeque};

/// `notfound` hashes kept per peer. Past the cap a peer may be asked again.
const LACKS_CAP: usize = 1024;

/// Announced hashes kept per peer: one full `headers` reply.
const ANNOUNCED_CAP: usize = rbitcoin_net::codec::MAX_HEADERS_RESULTS;

#[derive(Debug, Default)]
pub struct PeerChain {
    lacks: HashSet<BlockHash>,
    /// This peer's announced chain above the confirmed tip, oldest first.
    announced: VecDeque<BlockHash>,
    /// Height of `announced[0]`.
    from: u32,
    /// The peer announced headers, or was asked for them once.
    probed: bool,
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

    pub fn probed(&self) -> bool {
        self.probed
    }

    pub fn note_probed(&mut self) {
        self.probed = true;
    }

    /// Keep announced `headers`, the first at `height`, above the confirmed
    /// `tip`. A batch that overlaps or continues the kept run replaces it
    /// from where it starts; one that starts lower replaces it. One that
    /// leaves a gap above it is dropped: the run nearer the tip says more.
    pub fn note_headers(&mut self, tip: u32, height: u32, headers: &[Header]) {
        self.probed = true;
        let drop = tip.saturating_add(1).saturating_sub(self.from) as usize;
        self.announced.drain(..drop.min(self.announced.len()));
        self.from = self.from.max(tip.saturating_add(1));
        let skip = tip.saturating_add(1).saturating_sub(height) as usize;
        let Some(fresh) = headers.get(skip..).filter(|h| !h.is_empty()) else {
            return;
        };
        let start = height.saturating_add(skip as u32);
        let end = self.from.saturating_add(self.announced.len() as u32);
        if self.announced.is_empty() || start < self.from {
            self.announced.clear();
            self.from = start;
        } else if start > end {
            return;
        } else {
            self.announced.truncate((start - self.from) as usize);
        }
        self.announced.extend(fresh.iter().map(|h| h.block_hash()));
        self.announced.truncate(ANNOUNCED_CAP);
    }

    /// The peer's announced chain holds a different block than the path at
    /// `height`, or at its own top below it. Two chains that differ at one
    /// height differ above it, so the peer does not have the path block.
    pub fn left_path_by(&self, path: &HashMap<u32, BlockHash>, height: u32) -> bool {
        let Some(len) = (self.announced.len() as u32).checked_sub(1) else {
            return false;
        };
        if height < self.from {
            return false;
        }
        let at = height.min(self.from.saturating_add(len));
        let peer = self.announced[(at - self.from) as usize];
        path.get(&at).is_some_and(|p| *p != peer)
    }
}
