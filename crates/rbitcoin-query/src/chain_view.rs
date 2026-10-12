//! Header navigation, locators, height lookup, confirmed-read pin.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static HEIGHT_INDEX_LOCK_HELD: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
static INDEX_IO_UNDER_LOCK: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn reset_index_io_under_lock() {
    INDEX_IO_UNDER_LOCK.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn index_io_under_lock() -> u64 {
    INDEX_IO_UNDER_LOCK.load(Ordering::Relaxed)
}

#[cfg(test)]
struct IndexLockFlag;

#[cfg(test)]
impl IndexLockFlag {
    fn enter() -> Self {
        HEIGHT_INDEX_LOCK_HELD.with(|held| held.set(true));
        Self
    }
}

#[cfg(test)]
impl Drop for IndexLockFlag {
    fn drop(&mut self) {
        HEIGHT_INDEX_LOCK_HELD.with(|held| held.set(false));
    }
}

enum HeightIndexPlan {
    Current,
    Shrink { prev: u32 },
    Extend { prev: Option<u32> },
}

#[cfg(test)]
fn note_header_io_under_index_lock() {
    if HEIGHT_INDEX_LOCK_HELD.with(|held| held.get()) {
        INDEX_IO_UNDER_LOCK.fetch_add(1, Ordering::Relaxed);
    }
}

/// Header-store lookups in this process. Tests pin that a below-floor ignore
/// does not walk parents. Relaxed: the count is not a synchronization edge.
static HEADER_LOOKUP_N: AtomicU64 = AtomicU64::new(0);

#[doc(hidden)]
pub fn header_lookup_count() -> u64 {
    HEADER_LOOKUP_N.load(Ordering::Relaxed)
}

pub(crate) fn note_header_lookup() {
    HEADER_LOOKUP_N.fetch_add(1, Ordering::Relaxed);
}

/// Published best-chain prefix a confirmed-tx read was built against.
///
/// `still_live` is true while `confirmed[height]` is still this `header_fk`.
/// Tip **extension** leaves a shorter pin live (prefix-stable). Disconnect or
/// same-height replace of that height does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainView {
    pub height: Height,
    pub hash: [u8; 32],
    pub header_fk: Fk,
}

impl ChainView {
    pub fn still_live(&self, query: &Query) -> Result<bool, QueryError> {
        match query.store.confirmed.get(self.height)? {
            Some(fk) if fk == self.header_fk => Ok(true),
            _ => Ok(false),
        }
    }
}

/// Which published prefix a confirmed-tx read pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainViewKind {
    Tip,
    ScriptHash,
}

impl Query {
    /// Capture the published tip as a [`ChainView`], or `None` if the chain is empty.
    pub fn pin_chain_view(&self) -> Result<Option<ChainView>, QueryError> {
        let Some(height) = self.tip_height() else {
            return Ok(None);
        };
        let Some((_, rec)) = self.header_at_height(height)? else {
            return Ok(None);
        };
        self.pin_chain_view_at(&rec.hash)
    }

    /// Pin the last height SH can serve: durable watermark plus pending
    /// write-behind records (RAM). Pending fills the gap to live tip so
    /// mempool can drop confirmed txs without them vanishing from SH reads.
    /// `None` until the first enqueue or apply.
    pub fn pin_sh_chain_view(&self) -> Result<Option<ChainView>, QueryError> {
        let Some(h) = self.sh_visible_through_height() else {
            return Ok(None);
        };
        let Some((_, rec)) = self.header_at_height(Height(h))? else {
            return Ok(None);
        };
        self.pin_chain_view_at(&rec.hash)
    }

    /// As-of pin for SH reads: best-chain `hash` at or behind the visible SH
    /// height (durable + pending). Ahead of that is `None`.
    pub fn pin_sh_chain_view_at(&self, hash: &[u8; 32]) -> Result<Option<ChainView>, QueryError> {
        let Some(view) = self.pin_chain_view_at(hash)? else {
            return Ok(None);
        };
        let Some(max_h) = self.sh_visible_through_height() else {
            return Ok(None);
        };
        if view.height.0 > max_h {
            return Ok(None);
        }
        Ok(Some(view))
    }

    /// Pin live tip, visible SH, or a still-live ancestor (`asof`).
    pub fn pin_view(
        &self,
        kind: ChainViewKind,
        asof: Option<&[u8; 32]>,
    ) -> Result<Option<ChainView>, QueryError> {
        match (kind, asof) {
            (ChainViewKind::Tip, None) => self.pin_chain_view(),
            (ChainViewKind::Tip, Some(h)) => self.pin_chain_view_at(h),
            (ChainViewKind::ScriptHash, None) => self.pin_sh_chain_view(),
            (ChainViewKind::ScriptHash, Some(h)) => self.pin_sh_chain_view_at(h),
        }
    }

    /// Confirmed heights strictly above the SH watermark (`0` when caught up).
    pub fn sh_lag_heights(&self) -> u32 {
        match (self.tip_height(), self.sh_indexed_through_height()) {
            (None, _) => 0,
            (Some(tip), None) => tip.0.saturating_add(1),
            (Some(tip), Some(through)) => tip.0.saturating_sub(through),
        }
    }

    /// Pin a **best-chain** header hash (tip or buried). `None` if it is not
    /// `confirmed[height]` (unknown, archive-only orphan, or disconnected).
    pub fn pin_chain_view_at(&self, hash: &[u8; 32]) -> Result<Option<ChainView>, QueryError> {
        let Some(height) = self.height_of_hash(hash)? else {
            return Ok(None);
        };
        let Some(header_fk) = self.store.confirmed.get(height)? else {
            return Ok(None);
        };
        let rec = self.store.get_header(header_fk)?;
        if rec.hash != *hash {
            return Ok(None);
        }
        Ok(Some(ChainView {
            height,
            hash: rec.hash,
            header_fk,
        }))
    }

    /// Pin, run `f`, return the body if that pin is still published; else retry.
    ///
    /// Bound is 8. Empty chain is [`StoreError::NotFound`]. A pin that never
    /// stays live is [`StoreError::Stale`], not corruption.
    pub fn run_at_chain_view<T, F>(&self, f: F) -> Result<(ChainView, T), QueryError>
    where
        F: FnMut(&ChainView) -> Result<T, QueryError>,
    {
        self.run_at_view(ChainViewKind::Tip, f)
    }

    /// Pin by [`ChainViewKind`], run `f`, retry while that pin stays published.
    pub fn run_at_view<T, F>(
        &self,
        kind: ChainViewKind,
        mut f: F,
    ) -> Result<(ChainView, T), QueryError>
    where
        F: FnMut(&ChainView) -> Result<T, QueryError>,
    {
        const BOUND: u32 = 8;
        for _ in 0..BOUND {
            let Some(view) = self.pin_view(kind, None)? else {
                return Err(StoreError::NotFound);
            };
            let out = f(&view)?;
            if view.still_live(self)? {
                return Ok((view, out));
            }
        }
        Err(StoreError::Stale("chain view moved"))
    }

    pub fn header_at_height(
        &self,
        height: Height,
    ) -> Result<Option<(Fk, HeaderRecord)>, QueryError> {
        note_header_lookup();
        #[cfg(test)]
        note_header_io_under_index_lock();
        match self.store.confirmed.get(height)? {
            None => Ok(None),
            Some(fk) => Ok(Some((fk, self.store.get_header(fk)?))),
        }
    }

    /// Best-chain height of a header hash, if it is **confirmed** on the tip chain.
    ///
    /// Archive may contain orphan header rows (partial connect failures). Those are
    /// not reported here — only hashes reachable as `confirmed[height]`.
    ///
    /// Uses an in-process `hash → height` map (~60 MiB at mainnet tip). Any
    /// tip extend/shrink is incremental; a full `0..=tip` walk runs only on
    /// open or after [`Self::invalidate_height_by_hash_index`].
    pub fn height_of_hash(&self, hash: &[u8; 32]) -> Result<Option<Height>, QueryError> {
        let Some(tip) = self.tip_height() else {
            return Ok(None);
        };
        if let Some(indexed) = self.indexed_height(hash, tip.0) {
            return Ok(indexed);
        }
        if self.get_header_by_hash(hash)?.is_none() {
            return Ok(None);
        }
        self.ensure_height_by_hash_index(tip)?;
        Ok(self.with_height_index(|index| index.map.get(hash).copied().map(Height)))
    }

    /// Active-chain height of a hash already loaded from the header archive.
    ///
    /// Skips the extra header read in [`Self::height_of_hash`]. An ancestry
    /// walk then costs one `prev_fk` read per step plus this index.
    pub fn confirmed_height_of_hash(&self, hash: &[u8; 32]) -> Result<Option<Height>, QueryError> {
        let Some(tip) = self.tip_height() else {
            return Ok(None);
        };
        self.ensure_height_by_hash_index(tip)?;
        Ok(self.with_height_index(|index| index.map.get(hash).copied().map(Height)))
    }

    /// Ensure height index matches `tip`.
    ///
    /// Incremental for any `new_tip ≠ prev_tip`. Full walk only when the map
    /// is empty (`g.tip == None`). A walk that misses a height **above** the
    /// live published tip (stale snapshot / concurrent disconnect) retries
    /// the live tip. A miss **at or below** the live tip is Corrupt.
    pub(crate) fn ensure_height_by_hash_index(&self, tip: Height) -> Result<(), QueryError> {
        match self.ensure_height_by_hash_index_once(tip)? {
            None => Ok(()),
            Some(live) => self.ensure_height_by_hash_index_once(live).map(|_| ()),
        }
    }

    fn ensure_height_by_hash_index_once(&self, tip: Height) -> Result<Option<Height>, QueryError> {
        let plan = self.with_height_index(|index| {
            if index.tip == Some(tip.0) {
                HeightIndexPlan::Current
            } else if let Some(prev) = index.tip {
                if tip.0 < prev {
                    HeightIndexPlan::Shrink { prev }
                } else {
                    HeightIndexPlan::Extend { prev: Some(prev) }
                }
            } else {
                HeightIndexPlan::Extend { prev: None }
            }
        });
        match plan {
            HeightIndexPlan::Current => Ok(None),
            HeightIndexPlan::Shrink { prev } => {
                if self.shrink_height_index(tip.0, prev) {
                    Ok(None)
                } else {
                    Ok(self.tip_height())
                }
            }
            HeightIndexPlan::Extend { prev } => self.extend_index_from_headers(tip, prev),
        }
    }

    fn extend_index_from_headers(
        &self,
        tip: Height,
        prev: Option<u32>,
    ) -> Result<Option<Height>, QueryError> {
        let start = prev.map(|height| height.saturating_add(1)).unwrap_or(0);
        let mut rows = Vec::with_capacity((tip.0 - start) as usize + 1);
        for height in start..=tip.0 {
            let Some((_, rec)) = self.header_at_height(Height(height))? else {
                return self.stale_or_missing(height);
            };
            rows.push((rec.hash, height));
        }
        if self.extend_height_index(&rows, tip.0, prev, prev.is_none()) {
            if prev.is_none() {
                crate::note_confirm(&self.confirm_stats().height_index_full_n, 1);
                crate::note_confirm(
                    &self.confirm_stats().height_index_full_headers,
                    rows.len() as u64,
                );
            } else {
                crate::note_confirm(
                    &self.confirm_stats().height_index_delta_n,
                    rows.len() as u64,
                );
            }
            Ok(None)
        } else {
            Ok(self.tip_height())
        }
    }

    fn indexed_height(&self, hash: &[u8; 32], tip: u32) -> Option<Option<Height>> {
        self.with_height_index(|index| {
            if index.tip == Some(tip) {
                Some(index.map.get(hash).copied().map(Height))
            } else {
                None
            }
        })
    }

    pub(crate) fn with_height_index<R>(&self, f: impl FnOnce(&mut HeightByHashIndex) -> R) -> R {
        #[cfg(test)]
        let _held = IndexLockFlag::enter();
        let mut index = self
            .height_by_hash
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        f(&mut index)
    }

    pub(crate) fn extend_height_index(
        &self,
        rows: &[([u8; 32], u32)],
        new_tip: u32,
        expected: Option<u32>,
        clear: bool,
    ) -> bool {
        self.with_height_index(|index| {
            if index.tip == Some(new_tip) {
                return true;
            }
            if index.tip != expected {
                return false;
            }
            if clear {
                index.map.clear();
            }
            index.map.reserve(rows.len());
            for (hash, height) in rows {
                index.map.insert(*hash, *height);
            }
            index.tip = Some(new_tip);
            true
        })
    }

    fn shrink_height_index(&self, new_tip: u32, expected: u32) -> bool {
        self.with_height_index(|index| {
            if index.tip == Some(new_tip) {
                return true;
            }
            if index.tip != Some(expected) {
                return false;
            }
            let before = index.map.len() as u64;
            index.map.retain(|_, height| *height <= new_tip);
            crate::note_confirm(
                &self.confirm_stats().height_index_delta_n,
                before.saturating_sub(index.map.len() as u64),
            );
            index.tip = Some(new_tip);
            true
        })
    }

    fn stale_or_missing(&self, missing: u32) -> Result<Option<Height>, QueryError> {
        match self.tip_height() {
            Some(live) if live.0 < missing => Ok(Some(live)),
            None if missing > 0 => {
                self.invalidate_height_by_hash_index();
                Ok(None)
            }
            _ => Err(StoreError::Corrupt(
                "invariant: height_by_hash confirmed header missing",
            )),
        }
    }

    /// Leave `0..=through` indexed and a gap above it (tests).
    #[cfg(test)]
    pub(crate) fn leave_height_index_gap_for_test(&self, through: u32) {
        self.with_height_index(|index| {
            index.map.retain(|_, height| *height <= through);
            index.tip = Some(through);
        });
    }

    /// Drop height index (tests / multi-height reorg / offline confirmed rewrite).
    pub fn invalidate_height_by_hash_index(&self) {
        self.with_height_index(|index| {
            index.tip = None;
            index.map.clear();
        });
    }

    /// Wire header for a confirmed height (resolves prev hash from archive).
    pub fn wire_header_at_height(&self, height: Height) -> Result<BlockHeader, QueryError> {
        let (_fk, rec) = self.header_at_height(height)?.ok_or(StoreError::NotFound)?;
        self.wire_header_from_record(&rec)
    }

    pub(crate) fn wire_header_from_record(
        &self,
        rec: &HeaderRecord,
    ) -> Result<BlockHeader, QueryError> {
        self.wire_header_from_record_prev(rec, None)
    }

    /// Wire header with optional prev hash (cache package — avoids store get).
    pub(crate) fn wire_header_from_record_prev(
        &self,
        rec: &HeaderRecord,
        prev_hash: Option<[u8; 32]>,
    ) -> Result<BlockHeader, QueryError> {
        let prev_blockhash = if rec.prev_fk.is_null() {
            BlockHash::from_byte_array([0u8; 32])
        } else if let Some(h) = prev_hash {
            BlockHash::from_byte_array(h)
        } else {
            let prev = self.store.get_header(rec.prev_fk)?;
            BlockHash::from_byte_array(prev.hash)
        };
        Ok(wire_header(rec, prev_blockhash))
    }

    /// Reconstruct a full wire block from Class A archive by header hash
    /// (confirmed or not). Requires `header_txs` body.
    pub fn locator_hashes(&self) -> Result<Vec<BlockHash>, QueryError> {
        let Some(tip) = self.tip_height() else {
            return Ok(vec![BlockHash::from_byte_array([0u8; 32])]);
        };
        let mut out = Vec::new();
        let mut h = tip.0 as i64;
        let mut step = 1i64;
        while h >= 0 {
            let (_fk, rec) = self
                .header_at_height(Height(h as u32))?
                .ok_or(StoreError::NotFound)?;
            out.push(BlockHash::from_byte_array(rec.hash));
            if out.len() >= 10 {
                step *= 2;
            }
            h -= step;
        }
        if let Some((_fk, rec)) = self.header_at_height(Height::GENESIS)? {
            let g = BlockHash::from_byte_array(rec.hash);
            if out.last() != Some(&g) {
                out.push(g);
            }
        }
        Ok(out)
    }

    /// Headers on the best chain after the first matching locator entry, up to `limit` (max 2000).
    pub fn headers_after_locator(
        &self,
        locator: &[BlockHash],
        stop: BlockHash,
        limit: usize,
    ) -> Result<Vec<BlockHeader>, QueryError> {
        let Some(tip) = self.tip_height() else {
            return Ok(Vec::new());
        };
        let limit = limit.min(2000);
        // Empty locator: Core treats hashstop as a single-block request
        // (`p2p_sendheaders` null-locator). Unknown / zero stop → no headers.
        if locator.is_empty() {
            if stop.to_byte_array() == [0u8; 32] {
                return Ok(Vec::new());
            }
            return match self.height_of_hash(&stop.to_byte_array())? {
                Some(h) => Ok(vec![self.wire_header_at_height(h)?]),
                None => Ok(Vec::new()),
            };
        }
        let mut start = 0u32;
        'outer: for loc in locator {
            if loc.to_byte_array() == [0u8; 32] {
                start = 0;
                break;
            }
            if let Some(h) = self.height_of_hash(&loc.to_byte_array())? {
                start = h.0.saturating_add(1);
                break 'outer;
            }
        }
        // If no locator matched, Bitcoin peers typically start from genesis; we start at 0.
        let mut out = Vec::new();
        let mut h = start;
        while h <= tip.0 && out.len() < limit {
            let hdr = self.wire_header_at_height(Height(h))?;
            let hash = hdr.block_hash();
            out.push(hdr);
            if hash == stop && stop.to_byte_array() != [0u8; 32] {
                break;
            }
            h += 1;
        }
        Ok(out)
    }
}
