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
    /// The last reply was shorter than a full `headers` batch: the peer had
    /// nothing past its top.
    complete: bool,
    /// The peer's header at the first height where its announced chain
    /// differed from this node's chain (path or confirmed) when announced.
    split: Option<(u32, BlockHash)>,
    /// That height has left the path and this node's confirmed block there
    /// is a different one.
    split_confirmed: bool,
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

    /// Keep announced `headers`, the first at `height`, against this node's
    /// chain: the `path`, or a `confirmed` block at or below `tip`. The first
    /// height where they differ is where the peer leaves this chain; a later
    /// reply that agrees there clears it. Only heights above `tip` are kept,
    /// newest last: a reply that continues the kept run extends it, any other
    /// replaces it, and the oldest hashes past one full reply give way.
    pub fn note_headers(
        &mut self,
        path: &HashMap<u32, BlockHash>,
        confirmed: impl Fn(u32) -> Option<BlockHash>,
        tip: u32,
        height: u32,
        headers: &[Header],
    ) {
        self.probed = true;
        let hashes: Vec<BlockHash> = headers.iter().map(|h| h.block_hash()).collect();
        for (ht, hash) in (height..).zip(&hashes) {
            let on_path = path.get(&ht).copied();
            let Some(our) = on_path.or_else(|| (ht <= tip).then(|| confirmed(ht)).flatten()) else {
                continue;
            };
            if our == *hash {
                if self.split.is_some_and(|(at, _)| at == ht) {
                    self.split = None;
                }
                continue;
            }
            if self.split.is_none_or(|(at, _)| ht <= at) {
                self.split = Some((ht, *hash));
                self.split_confirmed = on_path.is_none();
            }
            break;
        }
        let drop = tip.saturating_add(1).saturating_sub(self.from) as usize;
        self.announced.drain(..drop.min(self.announced.len()));
        self.from = self.from.max(tip.saturating_add(1));
        let skip = tip.saturating_add(1).saturating_sub(height) as usize;
        let Some(fresh) = hashes.get(skip..).filter(|h| !h.is_empty()) else {
            return;
        };
        let start = height.saturating_add(skip as u32);
        let end = self.from.saturating_add(self.announced.len() as u32);
        if self.announced.is_empty() || start < self.from || start > end {
            self.announced.clear();
            self.from = start;
        } else {
            self.announced.truncate((start - self.from) as usize);
        }
        self.announced.extend(fresh.iter().copied());
        let over = self.announced.len().saturating_sub(ANNOUNCED_CAP);
        self.announced.drain(..over);
        self.from = self.from.saturating_add(over as u32);
        self.complete = headers.len() < ANNOUNCED_CAP;
    }

    /// The recorded split's height has left the path: compare it with this
    /// node's `confirmed` block there. The same block means this node took
    /// the peer's chain, so the split is gone.
    pub fn settle_split(
        &mut self,
        path: &HashMap<u32, BlockHash>,
        confirmed: impl Fn(u32) -> Option<BlockHash>,
    ) {
        let Some((at, hash)) = self.split else {
            return;
        };
        if path.contains_key(&at) {
            return;
        }
        match confirmed(at) {
            Some(ours) if ours == hash => self.split = None,
            ours => self.split_confirmed = ours.is_some(),
        }
    }

    /// The peer does not have the path block at `height`: its chain left this
    /// node's chain at or below it, it holds a different block there, or its
    /// whole chain ends below it. Two chains that differ at one height differ
    /// above it.
    pub fn left_path_by(&self, path: &HashMap<u32, BlockHash>, height: u32) -> bool {
        if let Some((at, hash)) = self.split {
            let differs = match path.get(&at) {
                Some(p) => *p != hash,
                None => self.split_confirmed,
            };
            if at <= height && differs {
                return true;
            }
        }
        let Some(len) = (self.announced.len() as u32).checked_sub(1) else {
            return false;
        };
        if height < self.from {
            return false;
        }
        let top = self.from.saturating_add(len);
        if height > top && self.complete {
            return true;
        }
        let at = height.min(top);
        let peer = self.announced[(at - self.from) as usize];
        path.get(&at).is_some_and(|p| *p != peer)
    }
}

/// This node's confirmed block at `height`.
pub fn confirmed_at(hub: &rbitcoin_net::chain::ChainHub, height: u32) -> Option<BlockHash> {
    use bitcoin::hashes::Hash;
    let (_, rec) = hub
        .query
        .header_at_height(rbitcoin_primitives::Height(height))
        .ok()
        .flatten()?;
    Some(BlockHash::from_byte_array(rec.hash))
}
