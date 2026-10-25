//! Cached health/RPC header heights. Only producers walk header ancestry.

use super::ChainHub;
use bitcoin::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::BlockHash;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

type HeaderPoint = (BlockHash, u32);

/// Reporting generation captured before loading or validating an IBD walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IbdHeaderEpoch(u64);

#[derive(Clone)]
struct IbdProof {
    epoch: IbdHeaderEpoch,
    anchors: Vec<(BlockHash, u32)>,
    stored: Option<(BlockHash, u32)>,
    safe_until: u32,
}

#[derive(Default)]
pub(super) struct HeaderReport {
    epoch: IbdHeaderEpoch,
    revision: u64,
    invalid: Arc<HashSet<BlockHash>>,
    stored: HashMap<BlockHash, HeaderPoint>,
    ibd: Option<IbdProof>,
    ibd_height: Option<u32>,
    dirty: bool,
    pub(super) height: u32,
}

struct ReportContext {
    epoch: IbdHeaderEpoch,
    revision: u64,
    invalid: Arc<HashSet<BlockHash>>,
    proved: HashMap<BlockHash, u32>,
}

#[derive(Default)]
struct IbdValidation {
    height: Option<u32>,
    stored: Option<(BlockHash, u32)>,
    safe_until: u32,
}

impl HeaderReport {
    pub(super) fn new(invalid: HashSet<BlockHash>) -> Self {
        Self {
            invalid: Arc::new(invalid),
            ..Self::default()
        }
    }

    fn refresh_height(&mut self) {
        self.height = self
            .stored
            .values()
            .map(|point| point.1)
            .chain(self.ibd_height)
            .max()
            .unwrap_or(0);
    }

    fn context(&self) -> ReportContext {
        let mut proved: HashMap<_, _> = self.stored.values().copied().collect();
        if let Some(proof) = &self.ibd {
            if proof.epoch == self.epoch {
                if let Some((hash, height)) = proof.stored {
                    proved.insert(hash, height);
                }
            }
        }
        ReportContext {
            epoch: self.epoch,
            revision: self.revision,
            invalid: Arc::clone(&self.invalid),
            proved,
        }
    }
}

impl ChainHub {
    /// Capture before loading or validating an IBD origin or resume point.
    pub fn ibd_header_epoch(&self) -> IbdHeaderEpoch {
        self.header_report.read().unwrap().epoch
    }

    /// Publish accepted base/checkpoint hashes in increasing height order.
    /// New invalid marks require a positive ancestry proof, not just checks of
    /// the endpoints. An unprovable look-ahead tail leaves its proved prefix.
    pub fn publish_ibd_header_tip(
        &self,
        epoch: IbdHeaderEpoch,
        anchors: Vec<(BlockHash, u32)>,
    ) -> Option<IbdHeaderEpoch> {
        let mut report = self.header_report.write().unwrap();
        if (anchors.is_empty() && epoch != report.epoch)
            || anchors.windows(2).any(|w| w[0].1 >= w[1].1)
        {
            return None;
        }
        let target = anchors.last().map(|a| a.1);
        let previous = report.ibd.clone();
        let reusable = previous.as_ref().filter(|p| {
            p.epoch == report.epoch
                && !report.dirty
                && (anchors.starts_with(&p.anchors) || p.anchors.starts_with(&anchors))
        });
        let fast = (epoch == report.epoch && report.invalid.is_empty())
            || reusable.is_some_and(|p| {
                target.is_none_or(|h| {
                    report.ibd_height.is_some_and(|old| h <= old)
                        || (report.ibd_height == p.anchors.last().map(|a| a.1) && h <= p.safe_until)
                })
            });
        let safe_until = if report.invalid.is_empty() {
            u32::MAX
        } else {
            reusable.map_or(0, |p| {
                if anchors.starts_with(&p.anchors) {
                    p.safe_until
                } else {
                    target.unwrap_or(0)
                }
            })
        };
        if fast {
            report.ibd_height = target;
        } else {
            report.ibd_height = reusable
                .and(report.ibd_height)
                .zip(target)
                .map(|(old, new)| old.min(new));
        }
        let current = report.epoch;
        report.ibd = (!anchors.is_empty()).then_some(IbdProof {
            epoch: current,
            anchors,
            stored: reusable.and_then(|p| p.stored),
            safe_until,
        });
        report.dirty |= !fast;
        if !fast {
            report.revision = report.revision.wrapping_add(1);
        }
        report.refresh_height();
        fast.then_some(current)
    }

    pub(super) fn invalidate_header_report(&self) {
        let invalid = self.invalidated.set.read().unwrap();
        let mut report = self.header_report.write().unwrap();
        report.epoch.0 = report.epoch.0.wrapping_add(1);
        report.invalid = Arc::new(invalid.clone());
        report.stored.clear();
        report.ibd_height = None;
        report.dirty = true;
        report.refresh_height();
    }

    pub(super) fn revalidate_header_report(&self) {
        self.invalidate_header_report();
        self.refresh_header_report();
    }

    /// Run on a blocking producer role, with the IBD coordination lock dropped.
    pub fn refresh_header_report(&self) {
        let (mut context, proof) = {
            let report = self.header_report.read().unwrap();
            if !report.dirty {
                return;
            }
            (report.context(), report.ibd.clone())
        };
        let candidates: Vec<_> = self.header_tips.read().unwrap().entries().collect();
        let valid: HashMap<_, _> = candidates
            .into_iter()
            .filter_map(|(hash, height)| {
                self.stored_header_prefix(hash, &context.invalid, &context.proved)
                    .filter(|point| {
                        if point.0 == hash {
                            point.1 == height
                        } else {
                            point.1 < height
                        }
                    })
                    .map(|point| (hash, point))
            })
            .collect();
        context.proved.extend(valid.values().copied());
        let checked = proof
            .as_ref()
            .map(|p| self.check_ibd_header_proof(&p.anchors, &context));
        let tips = self.header_tips.read().unwrap();
        let mut report = self.header_report.write().unwrap();
        if report.epoch != context.epoch || report.revision != context.revision {
            return;
        }
        for (hash, height) in valid {
            if tips.contains(&hash) {
                report.stored.insert(hash, height);
            }
        }
        let proof_matches = match (&proof, &report.ibd) {
            (Some(a), Some(b)) => a.anchors == b.anchors && a.epoch == b.epoch,
            (None, None) => true,
            _ => false,
        };
        if let (Some(proof), Some(checked)) = (proof, checked) {
            if report
                .ibd
                .as_ref()
                .is_some_and(|p| p.anchors == proof.anchors && p.epoch == proof.epoch)
            {
                report.ibd_height = checked.height;
                report.ibd = Some(IbdProof {
                    epoch: context.epoch,
                    anchors: proof.anchors,
                    stored: checked.stored,
                    safe_until: checked.safe_until,
                });
            }
        }
        report.refresh_height();
        report.dirty &= !proof_matches;
    }

    #[doc(hidden)]
    pub fn note_header_tip(&self, header: &Header) {
        let hash = header.block_hash();
        let registered_invalid = self.invalidated.set.read().unwrap().contains(&hash);
        if registered_invalid {
            {
                let mut report = self.header_report.write().unwrap();
                report.dirty = true;
                report.revision = report.revision.wrapping_add(1);
            }
            self.refresh_header_report();
        }
        if self.is_connected(&hash) {
            self.forget_reported_header_tip(&hash);
            return;
        }
        let prev = header.prev_blockhash;
        let connected = self.is_connected(&prev);
        let height = if connected {
            self.query
                .published_height_of_hash(prev.as_byte_array())
                .map(|h| h.0.saturating_add(1))
        } else {
            self.header_tips
                .read()
                .unwrap()
                .height_of(&prev)
                .map(|h| h.saturating_add(1))
        };
        let height = height.or_else(|| {
            let context = self.header_report.read().unwrap().context();
            self.proved_stored_header_height(hash, &HashSet::new(), &context.proved)
        });
        let Some(height) = height else {
            return;
        };
        let (epoch, parent_proof) = {
            let report = self.header_report.read().unwrap();
            (report.epoch, report.stored.get(&prev).copied())
        };
        let directly_invalid = {
            let invalid = self.invalidated.set.read().unwrap();
            invalid.contains(&hash) || invalid.contains(&prev)
        };
        let proof = if !directly_invalid && (connected || parent_proof.is_some_and(|p| p.0 == prev))
        {
            Some((hash, height))
        } else if let Some(parent) = parent_proof {
            Some(parent)
        } else {
            let context = self.header_report.read().unwrap().context();
            self.stored_header_prefix(hash, &context.invalid, &context.proved)
        };
        let mut tips = self.header_tips.write().unwrap();
        tips.remove(&prev);
        if !tips.contains(&hash) && tips.len() >= 128 {
            tips.evict_one();
        }
        tips.insert(hash, prev, height);
        let mut report = self.header_report.write().unwrap();
        let stale = epoch != report.epoch;
        if !stale {
            if let Some(proof) = proof {
                report.stored.insert(hash, proof);
            }
        }
        report.stored.retain(|hash, _| tips.contains(hash));
        report.refresh_height();
        drop(report);
        drop(tips);
        if stale {
            self.refresh_stored_header_tip(hash, height);
        }
    }

    pub(super) fn forget_reported_header_tip(&self, hash: &BlockHash) {
        self.header_tips.write().unwrap().remove(hash);
        let mut report = self.header_report.write().unwrap();
        report.stored.remove(hash);
        report.refresh_height();
    }

    fn refresh_stored_header_tip(&self, hash: BlockHash, height: u32) {
        let context = self.header_report.read().unwrap().context();
        let Some(prefix) = self
            .stored_header_prefix(hash, &context.invalid, &context.proved)
            .filter(|p| {
                if p.0 == hash {
                    p.1 == height
                } else {
                    p.1 < height
                }
            })
        else {
            return;
        };
        let tips = self.header_tips.read().unwrap();
        let mut report = self.header_report.write().unwrap();
        if report.epoch == context.epoch && tips.contains(&hash) {
            report.stored.insert(hash, prefix);
            report.refresh_height();
        }
    }

    fn proved_stored_header_height(
        &self,
        hash: BlockHash,
        invalid: &HashSet<BlockHash>,
        proved: &HashMap<BlockHash, u32>,
    ) -> Option<u32> {
        self.stored_header_prefix(hash, invalid, proved)
            .filter(|point| point.0 == hash)
            .map(|point| point.1)
    }

    fn stored_header_prefix(
        &self,
        hash: BlockHash,
        invalid: &HashSet<BlockHash>,
        proved: &HashMap<BlockHash, u32>,
    ) -> Option<HeaderPoint> {
        let (mut fk, mut rec) = self.query.get_header_by_hash(hash.as_byte_array()).ok()??;
        let mut prefix = hash;
        let mut depth = 0u32;
        loop {
            let hash = BlockHash::from_byte_array(rec.hash);
            if invalid.contains(&hash) {
                if rec.prev_fk.is_null() || rec.prev_fk.0 >= fk.0 {
                    return None;
                }
                fk = rec.prev_fk;
                rec = self.query.get_header(fk).ok()?;
                prefix = BlockHash::from_byte_array(rec.hash);
                depth = 0;
                continue;
            }
            if let Some(height) = proved
                .get(&hash)
                .copied()
                .or_else(|| self.query.published_height_of_hash(&rec.hash).map(|h| h.0))
            {
                return Some((prefix, height.checked_add(depth)?));
            }
            if rec.prev_fk.is_null() {
                return (hash == self.params.genesis_hash).then_some((prefix, depth));
            }
            if rec.prev_fk.0 >= fk.0 {
                return None;
            }
            fk = rec.prev_fk;
            rec = self.query.get_header(fk).ok()?;
            depth = depth.checked_add(1)?;
        }
    }

    fn stored_hash_at(&self, tip: BlockHash, height: u32, target: u32) -> Option<BlockHash> {
        let (mut fk, mut rec) = self.query.get_header_by_hash(tip.as_byte_array()).ok()??;
        for _ in target..height {
            if rec.prev_fk.is_null() || rec.prev_fk.0 >= fk.0 {
                return None;
            }
            fk = rec.prev_fk;
            rec = self.query.get_header(fk).ok()?;
        }
        Some(BlockHash::from_byte_array(rec.hash))
    }

    fn check_ibd_header_proof(
        &self,
        anchors: &[(BlockHash, u32)],
        context: &ReportContext,
    ) -> IbdValidation {
        let Some(&(_, height)) = anchors.last() else {
            return IbdValidation::default();
        };
        let Some(&(stored, stored_height)) = anchors.iter().rev().find(|(hash, _)| {
            self.query
                .get_header_by_hash(hash.as_byte_array())
                .ok()
                .flatten()
                .is_some()
        }) else {
            return IbdValidation::default();
        };
        let valid_prefix = self
            .stored_header_prefix(stored, &context.invalid, &context.proved)
            .or_else(|| {
                anchors.iter().rev().find_map(|&(hash, height)| {
                    (self.proved_stored_header_height(hash, &context.invalid, &context.proved)
                        == Some(height))
                    .then_some((hash, height))
                })
            });
        let Some(valid_prefix) = valid_prefix else {
            return IbdValidation::default();
        };
        if valid_prefix != (stored, stored_height) {
            return IbdValidation {
                height: Some(valid_prefix.1),
                stored: Some(valid_prefix),
                safe_until: valid_prefix.1,
            };
        }
        let prefix = IbdValidation {
            height: Some(stored_height),
            stored: Some((stored, stored_height)),
            safe_until: stored_height,
        };
        let mut safe_until = u32::MAX;
        let none = HashSet::new();
        for &hash in context.invalid.iter() {
            if let Some((_, invalid_height)) = anchors.iter().find(|a| a.0 == hash) {
                safe_until = safe_until.min(invalid_height.saturating_sub(1));
                continue;
            }
            let Some(invalid_height) =
                self.proved_stored_header_height(hash, &none, &context.proved)
            else {
                safe_until = safe_until.min(stored_height);
                continue;
            };
            if invalid_height <= stored_height {
                continue;
            }
            match self.stored_hash_at(hash, invalid_height, stored_height) {
                Some(ancestor) if ancestor != stored => {}
                Some(_) => safe_until = safe_until.min(invalid_height.saturating_sub(1)),
                None => safe_until = safe_until.min(stored_height),
            }
        }
        IbdValidation {
            height: Some(height.min(safe_until)),
            safe_until,
            ..prefix
        }
    }
}
