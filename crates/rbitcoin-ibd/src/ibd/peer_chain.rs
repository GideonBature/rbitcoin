//! What one IBD peer has shown of its chain.
//!
//! Core asks a peer only for blocks on that peer's best known chain
//! (`pindexBestKnownBlock`). A peer that answered `notfound` for a block is
//! not asked for it again.

use bitcoin::BlockHash;
use std::collections::HashSet;

/// `notfound` hashes kept per peer. Past the cap a peer may be asked again.
const LACKS_CAP: usize = 1024;

#[derive(Debug, Default)]
pub struct PeerChain {
    lacks: HashSet<BlockHash>,
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
}
