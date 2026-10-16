//! In-memory wire block cache for P2P serve and ordered sync.
//!
//! Full block bodies are kept only for a recent tip window (default
//! [`DEFAULT_BODY_DEPTH`], compact/`getblocktxn` serve). Best-chain **hashes**
//! are retained for locator construction without holding decoded IBD history.

use bitcoin::block::{Block, Header};
use bitcoin::hashes::Hash;
use bitcoin::BlockHash;
use std::collections::HashMap;
use std::sync::RwLock;

/// How many recent full block bodies to retain (compact/`getblocktxn` serve
/// depths are 5 and 10; hashes stay for locators).
pub const DEFAULT_BODY_DEPTH: usize = 16;

#[derive(Debug)]
pub struct BlockCache {
    inner: RwLock<Inner>,
    /// Max full bodies in `by_hash` (hashes in `chain` are always kept).
    body_depth: usize,
}

#[derive(Debug, Default)]
struct Inner {
    by_hash: HashMap<BlockHash, Block>,
    /// Best-chain hashes by height (contiguous from 0).
    chain: Vec<BlockHash>,
}

impl Default for BlockCache {
    fn default() -> Self {
        Self::with_body_depth(DEFAULT_BODY_DEPTH)
    }
}

impl BlockCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_body_depth(body_depth: usize) -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
            body_depth: body_depth.max(1),
        }
    }

    pub fn tip_height(&self) -> Option<u32> {
        let g = self.inner.read().unwrap();
        if g.chain.is_empty() {
            None
        } else {
            Some((g.chain.len() - 1) as u32)
        }
    }

    pub fn tip_hash(&self) -> Option<BlockHash> {
        self.inner.read().unwrap().chain.last().copied()
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().chain.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().unwrap().chain.is_empty()
    }

    pub fn body_count(&self) -> usize {
        self.inner.read().unwrap().by_hash.len()
    }

    pub fn get_block(&self, hash: &BlockHash) -> Option<Block> {
        self.inner.read().unwrap().by_hash.get(hash).cloned()
    }

    pub fn get_header(&self, hash: &BlockHash) -> Option<Header> {
        self.inner
            .read()
            .unwrap()
            .by_hash
            .get(hash)
            .map(|b| b.header)
    }

    pub fn hash_at_height(&self, height: u32) -> Option<BlockHash> {
        self.inner
            .read()
            .unwrap()
            .chain
            .get(height as usize)
            .copied()
    }

    pub fn header_at_height(&self, height: u32) -> Option<Header> {
        let g = self.inner.read().unwrap();
        let h = g.chain.get(height as usize)?;
        g.by_hash.get(h).map(|b| b.header)
    }

    /// Drop all blocks above `height` (keep 0..=height). No-op if cache shorter.
    pub fn truncate_to_height(&self, height: u32) {
        let mut g = self.inner.write().unwrap();
        let keep = (height as usize).saturating_add(1);
        if g.chain.len() <= keep {
            return;
        }
        let remove: Vec<BlockHash> = g.chain[keep..].to_vec();
        g.chain.truncate(keep);
        for h in remove {
            g.by_hash.remove(&h);
        }
    }

    /// Clear entire cache (e.g. after full reorg from genesis).
    pub fn clear(&self) {
        let mut g = self.inner.write().unwrap();
        g.by_hash.clear();
        g.chain.clear();
    }

    /// Append a block that extends the best chain (or becomes genesis at height 0).
    ///
    /// Evicts full bodies older than `body_depth` while keeping the hash chain.
    pub fn push_best(&self, block: Block) -> Result<(), &'static str> {
        let hash = block.block_hash();
        let mut g = self.inner.write().unwrap();
        if g.chain.is_empty() {
            g.by_hash.insert(hash, block);
            g.chain.push(hash);
            return Ok(());
        }
        let tip = *g.chain.last().unwrap();
        if block.header.prev_blockhash != tip {
            return Err("block does not extend best chain");
        }
        g.by_hash.insert(hash, block);
        g.chain.push(hash);
        let extra = g.chain.len().saturating_sub(self.body_depth);
        if extra != 0 {
            let stale: Vec<BlockHash> = g.chain[..extra].to_vec();
            for h in stale {
                g.by_hash.remove(&h);
            }
        }
        Ok(())
    }

    /// Locator hashes newest-first for getheaders.
    pub fn locator(&self) -> Vec<BlockHash> {
        let g = self.inner.read().unwrap();
        if g.chain.is_empty() {
            return vec![BlockHash::from_byte_array([0u8; 32])];
        }
        let mut out = Vec::new();
        let mut i = g.chain.len() as i64 - 1;
        let mut step = 1i64;
        while i >= 0 {
            out.push(g.chain[i as usize]);
            if out.len() >= 10 {
                step *= 2;
            }
            i -= step;
        }
        // Always include genesis
        if let Some(g0) = g.chain.first() {
            if out.last() != Some(g0) {
                out.push(*g0);
            }
        }
        out
    }

    /// Headers after the best common locator hash, up to Core `MAX_HEADERS_RESULTS` (2000).
    ///
    /// Only returns headers still present as full bodies in the tip window; callers
    /// that need deeper history should use the store reconstruct path.
    pub fn headers_after_locator(&self, locator: &[BlockHash], stop: BlockHash) -> Vec<Header> {
        let g = self.inner.read().unwrap();
        if g.chain.is_empty() {
            return Vec::new();
        }
        let mut start = 0usize;
        'outer: for loc in locator {
            if loc.to_byte_array() == [0u8; 32] {
                start = 0;
                break;
            }
            for (i, h) in g.chain.iter().enumerate() {
                if h == loc {
                    start = i + 1;
                    break 'outer;
                }
            }
        }
        let mut out = Vec::new();
        for h in g
            .chain
            .iter()
            .skip(start)
            .take(crate::codec::MAX_HEADERS_RESULTS)
        {
            if let Some(b) = g.by_hash.get(h) {
                out.push(b.header);
                if *h == stop && stop.to_byte_array() != [0u8; 32] {
                    break;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_body_depth_is_compact_serve_window() {
        assert_eq!(DEFAULT_BODY_DEPTH, 16);
    }

    fn extend(prev: BlockHash, n: u32) -> Block {
        use bitcoin::block::{Header, Version};
        use bitcoin::CompactTarget;
        Block {
            header: Header {
                version: Version::from_consensus(1),
                prev_blockhash: prev,
                merkle_root: bitcoin::TxMerkleNode::from_byte_array([n as u8; 32]),
                time: n,
                bits: CompactTarget::from_consensus(0x207f_ffff),
                nonce: n,
            },
            txdata: vec![],
        }
    }

    #[test]
    fn bodies_stay_for_a_full_window_and_only_the_tail_after_that() {
        let cache = BlockCache::with_body_depth(3);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.body_count(), 0);
        let mut prev = BlockHash::from_byte_array([0u8; 32]);
        let mut hashes = Vec::new();
        for n in 1..=3 {
            let block = extend(prev, n);
            let hash = block.block_hash();
            cache.push_best(block).unwrap();
            hashes.push(hash);
            prev = hash;
        }
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.body_count(), 3, "chain length == depth drops nothing");
        assert!(cache.get_block(&hashes[0]).is_some());
        let block = extend(prev, 4);
        let h4 = block.block_hash();
        cache.push_best(block).unwrap();
        hashes.push(h4);
        prev = h4;
        assert!(cache.get_block(&hashes[0]).is_none());
        assert_eq!(cache.body_count(), 3);
        let block = extend(prev, 5);
        let h5 = block.block_hash();
        cache.push_best(block).unwrap();
        hashes.push(h5);
        assert_eq!(cache.len(), 5);
        assert_eq!(cache.body_count(), 3);
        assert!(cache.get_block(&hashes[0]).is_none());
        assert!(cache.get_block(&hashes[1]).is_none());
        assert!(cache.get_block(&hashes[2]).is_some());
        assert!(cache.get_block(&hashes[3]).is_some());
        assert!(cache.get_block(&hashes[4]).is_some());
    }

    #[test]
    fn locator_doubles_its_step_and_keeps_genesis_once() {
        let cache = BlockCache::with_body_depth(32);
        assert_eq!(cache.locator(), vec![BlockHash::from_byte_array([0u8; 32])]);
        let mut prev = BlockHash::from_byte_array([0u8; 32]);
        let mut hashes = Vec::new();
        for n in 1..=20 {
            let block = extend(prev, n);
            let hash = block.block_hash();
            cache.push_best(block).unwrap();
            hashes.push(hash);
            prev = hash;
        }
        let loc = cache.locator();
        let at = |h: usize| hashes[h];
        assert_eq!(
            loc,
            vec![
                at(19),
                at(18),
                at(17),
                at(16),
                at(15),
                at(14),
                at(13),
                at(12),
                at(11),
                at(10),
                at(8),
                at(4),
                at(0),
            ]
        );
        assert_eq!(cache.locator().last().copied(), Some(at(0)));
        assert_eq!(cache.locator().iter().filter(|h| **h == at(0)).count(), 1);
    }

    #[test]
    fn headers_after_locator_start_at_the_next_block_and_honor_stop() {
        let cache = BlockCache::with_body_depth(32);
        let mut prev = BlockHash::from_byte_array([0u8; 32]);
        let mut headers = Vec::new();
        let mut hashes = Vec::new();
        for n in 1..=5 {
            let block = extend(prev, n);
            let hash = block.block_hash();
            headers.push(block.header);
            cache.push_best(block).unwrap();
            hashes.push(hash);
            prev = hash;
        }
        let null = BlockHash::from_byte_array([0u8; 32]);
        let all = cache.headers_after_locator(&[null], null);
        assert_eq!(all, headers);
        let from_mid = cache.headers_after_locator(&[hashes[1]], null);
        assert_eq!(from_mid, headers[2..]);
        // A null locator entry wins over a later real hash (start at genesis).
        let null_then_tip = cache.headers_after_locator(&[null, hashes[3]], null);
        assert_eq!(null_then_tip, headers);
        let stopped = cache.headers_after_locator(&[null], hashes[2]);
        assert_eq!(stopped, headers[..=2]);
        assert!(cache
            .headers_after_locator(&[hashes[4]], hashes[4])
            .is_empty());
    }
}
