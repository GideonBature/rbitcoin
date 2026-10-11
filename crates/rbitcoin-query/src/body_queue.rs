//! Body queue lock domains.
//!
//! Three locks, never nested across domains:
//!
//! - **Height stripes** hold the raw/promoted rows. Lookup reads and getdata
//!   enqueue of *different* heights do not share a stripe.
//! - **Hash index** answers getdata `has_hash`. Lookup reads
//!   (`hash_at_height`, `raw_payload`, `wave_intake`, `promote_wave`) never
//!   take it. Row removal takes one hash stripe only long enough to drop that
//!   hash, and only while also holding that height stripe (hash stripe first).
//! - **Decoded stash** is lookup's promoted wires. Getdata enqueue and
//!   `has_hash` never take it.
//!
//! Input counts are stamped by the caller before [`BodyQueue::enqueue_vec`].
//! This type does not decode blocks.
//!
//! Lock order when two domains are required: hash stripe, then that height
//! stripe, and release both before the decoded stash. Never the reverse.

use std::cmp::Ordering as CmpOrd;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use rbitcoin_store::{BlockQueue, QueuedBlockMeta, StoreError, TakenRaw};

use crate::resolved_wire::{BlockQueueWaveIntake, ResolvedWire};

/// Height stripes. Consecutive heights land on different stripes so a lookup
/// wave and the next getdata accept do not share a row lock.
pub(crate) const STRIPES: usize = 16;

pub(crate) fn height_stripe(height: u32) -> usize {
    (height as usize) % STRIPES
}

fn hash_stripe(hash: &[u8; 32]) -> usize {
    (hash[0] as usize) % STRIPES
}

type HashIndex = HashMap<[u8; 32], Vec<u32>>;

pub(crate) struct BodyQueue {
    height: [Mutex<BlockQueue>; STRIPES],
    by_hash: [Mutex<HashIndex>; STRIPES],
    resolved: Mutex<HashMap<u32, ResolvedWire>>,
    bytes: AtomicU64,
    count: AtomicUsize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap()
}

fn remember(index: &mut HashIndex, hash: [u8; 32], height: u32) {
    let heights = index.entry(hash).or_default();
    if !heights.contains(&height) {
        heights.push(height);
    }
}

fn forget(index: &mut HashIndex, hash: &[u8; 32], height: u32) {
    let Some(heights) = index.get_mut(hash) else {
        return;
    };
    heights.retain(|h| *h != height);
    if heights.is_empty() {
        index.remove(hash);
    }
}

impl BodyQueue {
    pub(crate) fn open(store_path: &Path) -> Result<Self, StoreError> {
        let mut queues = Vec::with_capacity(STRIPES);
        queues.push(Mutex::new(BlockQueue::open_or_create(store_path)?));
        for _ in 1..STRIPES {
            queues.push(Mutex::new(BlockQueue::empty()));
        }
        let height = match <[Mutex<BlockQueue>; STRIPES]>::try_from(queues) {
            Ok(slots) => slots,
            Err(_) => unreachable!("height stripe count"),
        };
        Ok(Self {
            height,
            by_hash: std::array::from_fn(|_| Mutex::new(HashMap::new())),
            resolved: Mutex::new(HashMap::new()),
            bytes: AtomicU64::new(0),
            count: AtomicUsize::new(0),
        })
    }

    fn note_delta(&self, before_b: u64, after_b: u64, before_c: usize, after_c: usize) {
        match after_b.cmp(&before_b) {
            CmpOrd::Greater => {
                self.bytes.fetch_add(after_b - before_b, Ordering::Relaxed);
            }
            CmpOrd::Less => {
                self.bytes.fetch_sub(before_b - after_b, Ordering::Relaxed);
            }
            CmpOrd::Equal => {}
        }
        match after_c.cmp(&before_c) {
            CmpOrd::Greater => {
                self.count.fetch_add(after_c - before_c, Ordering::Relaxed);
            }
            CmpOrd::Less => {
                self.count.fetch_sub(before_c - after_c, Ordering::Relaxed);
            }
            CmpOrd::Equal => {}
        }
    }

    /// Hash stripe then height stripe. The closure sees both so enqueue and
    /// removal publish the row and the hash together.
    fn hash_then_height<R>(
        &self,
        hash: &[u8; 32],
        height: u32,
        f: impl FnOnce(&mut HashIndex, &mut BlockQueue) -> R,
    ) -> R {
        let mut hashes = lock(&self.by_hash[hash_stripe(hash)]);
        let mut rows = lock(&self.height[height_stripe(height)]);
        let before_b = rows.bytes();
        let before_c = rows.count();
        let out = f(&mut hashes, &mut rows);
        self.note_delta(before_b, rows.bytes(), before_c, rows.count());
        out
    }

    fn with_height_mut<R>(&self, height: u32, f: impl FnOnce(&mut BlockQueue) -> R) -> R {
        let mut rows = lock(&self.height[height_stripe(height)]);
        let before_b = rows.bytes();
        let before_c = rows.count();
        let out = f(&mut rows);
        self.note_delta(before_b, rows.bytes(), before_c, rows.count());
        out
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    pub(crate) fn bytes_and_count(&self) -> (u64, usize) {
        (self.bytes(), self.count())
    }

    pub(crate) fn max_height(&self) -> Option<u32> {
        let mut max = None;
        for stripe in &self.height {
            let rows = lock(stripe);
            max = max.max(rows.max_height());
        }
        max
    }

    pub(crate) fn id_for_height(&self, height: u32) -> Option<u64> {
        lock(&self.height[height_stripe(height)]).id_for_height(height)
    }

    pub(crate) fn enqueue_vec(
        &self,
        height: u32,
        hash: [u8; 32],
        header_fk: u64,
        owned: Vec<u8>,
        n_inputs: u32,
        sender: Option<u64>,
    ) -> Result<u64, StoreError> {
        self.hash_then_height(&hash, height, |index, rows| {
            if let Some(id) = rows.id_for_height(height) {
                return Ok(id);
            }
            let id = rows.enqueue_vec(height, hash, header_fk, owned, n_inputs, sender)?;
            remember(index, hash, height);
            Ok(id)
        })
    }

    pub(crate) fn dequeue_height(&self, height: u32) -> Result<usize, StoreError> {
        let Some(hash) = self.hash_at_height(height) else {
            lock(&self.resolved).remove(&height);
            return Ok(0);
        };
        let n = self.hash_then_height(&hash, height, |index, rows| {
            if rows.hash_at_height(height) != Some(hash) {
                return Ok::<usize, StoreError>(0);
            }
            let n = rows.dequeue_height(height)?;
            forget(index, &hash, height);
            Ok(n)
        })?;
        lock(&self.resolved).remove(&height);
        Ok(n)
    }

    pub(crate) fn list_meta(&self) -> Vec<QueuedBlockMeta> {
        let mut out = Vec::new();
        for stripe in &self.height {
            out.extend(lock(stripe).list_meta());
        }
        out.sort_by_key(|m| (m.height, m.id));
        out
    }

    pub(crate) fn queued_heights(&self) -> std::collections::BTreeSet<u32> {
        let mut out = std::collections::BTreeSet::new();
        for stripe in &self.height {
            for h in lock(stripe).heights() {
                out.insert(h);
            }
        }
        out
    }

    pub(crate) fn unresolved_heights(
        &self,
        start: u32,
        skip: &HashSet<u32>,
        cap: usize,
    ) -> Vec<u32> {
        if cap == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut h = start;
        while out.len() < cap {
            let take = {
                let rows = lock(&self.height[height_stripe(h)]);
                if !rows.contains_height(h) {
                    None
                } else if rows.is_resolve_complete(h) || skip.contains(&h) {
                    Some(false)
                } else {
                    Some(true)
                }
            };
            let Some(include) = take else {
                break;
            };
            if include {
                out.push(h);
            }
            h = match h.checked_add(1) {
                Some(n) => n,
                None => break,
            };
        }
        out
    }

    pub(crate) fn payload(&self, height: u32) -> Result<Option<Vec<u8>>, StoreError> {
        let rows = lock(&self.height[height_stripe(height)]);
        Ok(rows.get_by_height(height)?.map(|q| q.payload))
    }

    pub(crate) fn raw_payload(&self, height: u32) -> Option<std::sync::Arc<Vec<u8>>> {
        lock(&self.height[height_stripe(height)]).raw_payload(height)
    }

    pub(crate) fn take_raw_clone_n(&self) -> u64 {
        let mut n = 0u64;
        for stripe in &self.height {
            n = n.saturating_add(lock(stripe).take_raw_clone_n());
        }
        n
    }

    pub(crate) fn payload_by_hash(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>, StoreError> {
        use bitcoin::consensus::encode::serialize;
        let heights = {
            let index = lock(&self.by_hash[hash_stripe(hash)]);
            index.get(hash).cloned().unwrap_or_default()
        };
        for height in heights {
            let raw = {
                let rows = lock(&self.height[height_stripe(height)]);
                if rows.hash_at_height(height) != Some(*hash) {
                    continue;
                }
                rows.raw_payload(height)
            };
            if let Some(raw) = raw {
                return Ok(Some((*raw).clone()));
            }
            if let Some(wire) = lock(&self.resolved).get(&height) {
                return Ok(Some(serialize(wire.block.as_ref())));
            }
        }
        Ok(None)
    }

    pub(crate) fn has_hash(&self, hash: &[u8; 32]) -> bool {
        let index = lock(&self.by_hash[hash_stripe(hash)]);
        index.get(hash).is_some_and(|heights| !heights.is_empty())
    }

    pub(crate) fn contains_height(&self, height: u32) -> bool {
        lock(&self.height[height_stripe(height)]).contains_height(height)
    }

    pub(crate) fn take_raw(&self, height: u32) -> Option<TakenRaw> {
        let hash = self.hash_at_height(height)?;
        let taken = self.hash_then_height(&hash, height, |index, rows| {
            if rows.hash_at_height(height) != Some(hash) {
                return None;
            }
            let taken = rows.take_raw(height)?;
            forget(index, &hash, height);
            Some(taken)
        });
        if taken.is_some() {
            lock(&self.resolved).remove(&height);
        }
        taken
    }

    pub(crate) fn hash_at_height(&self, height: u32) -> Option<[u8; 32]> {
        lock(&self.height[height_stripe(height)]).hash_at_height(height)
    }

    pub(crate) fn mark_resolve_complete(&self, height: u32) -> Result<(), StoreError> {
        self.with_height_mut(height, |rows| rows.mark_resolve_complete(height))
    }

    pub(crate) fn is_resolve_complete(&self, height: u32) -> bool {
        lock(&self.height[height_stripe(height)]).is_resolve_complete(height)
    }

    pub(crate) fn wave_intake(&self, heights: &[u32]) -> BlockQueueWaveIntake {
        let mut out = BlockQueueWaveIntake::default();
        let mut promoted = Vec::new();
        for &h in heights {
            let rows = lock(&self.height[height_stripe(h)]);
            if rows.is_resolve_complete(h) {
                continue;
            }
            if rows.has_raw(h) {
                out.raw.push((
                    h,
                    rows.input_count_at(h).unwrap_or(0),
                    rows.header_fk_at(h).unwrap_or(0),
                    rows.sender_at(h),
                ));
            } else if rows.contains_height(h) {
                promoted.push(h);
            }
        }
        if !promoted.is_empty() {
            let stash = lock(&self.resolved);
            for h in promoted {
                if let Some(wire) = stash.get(&h) {
                    out.resolved.push((h, wire.clone()));
                }
            }
        }
        out
    }

    pub(crate) fn promote_wave(
        &self,
        items: Vec<(u32, ResolvedWire, u64)>,
    ) -> Result<usize, StoreError> {
        {
            let mut stash = lock(&self.resolved);
            for (height, wire, _) in &items {
                stash.insert(*height, wire.clone());
            }
        }
        let mut n = 0usize;
        for (height, _, charge) in &items {
            n += self.with_height_mut(*height, |rows| rows.promote_wave(&[(*height, *charge)]))?;
        }
        Ok(n)
    }

    pub(crate) fn resolved(&self, height: u32) -> Option<ResolvedWire> {
        lock(&self.resolved).get(&height).cloned()
    }

    pub(crate) fn drop_resolved_from(&self, height: u32) {
        lock(&self.resolved).retain(|&h, _| h < height);
    }

    pub(crate) fn promoted_count(&self) -> usize {
        let mut n = 0usize;
        for stripe in &self.height {
            n = n.saturating_add(lock(stripe).promoted_count());
        }
        n
    }

    pub(crate) fn mark_resolve_complete_wave(&self, heights: &[u32]) -> Result<usize, StoreError> {
        let mut n = 0usize;
        for &h in heights {
            n += self.with_height_mut(h, |rows| rows.mark_resolve_complete_wave(&[h]))?;
        }
        Ok(n)
    }

    #[cfg(test)]
    pub(crate) fn hold_resolved(&self) -> MutexGuard<'_, HashMap<u32, ResolvedWire>> {
        lock(&self.resolved)
    }

    #[cfg(test)]
    pub(crate) fn hold_hash_index(&self) -> HashIndexHold<'_> {
        HashIndexHold {
            _guards: self.by_hash.iter().map(lock).collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn hold_height_stripe(&self, height: u32) -> MutexGuard<'_, BlockQueue> {
        lock(&self.height[height_stripe(height)])
    }
}

#[cfg(test)]
pub(crate) struct HashIndexHold<'a> {
    _guards: Vec<MutexGuard<'a, HashIndex>>,
}
