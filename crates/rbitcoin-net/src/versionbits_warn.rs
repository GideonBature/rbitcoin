//! Unknown-versionbits activation warnings (Core `WarningBitsConditionChecker`).
//!
//! Mainnet: period 2016, threshold 1815 (90%). Regtest/testnets: period =
//! difficulty interval, threshold = 75% of period. Blocks below the network's
//! `MinBIP9WarningHeight` are not counted. When a completed period has
//! ≥threshold counted blocks signalling an unassigned bit with BIP9 top bits,
//! the next tip reports `Unknown new rules activated (versionbit N)`.

use rbitcoin_primitives::{Height, Network};
use rbitcoin_query::Query;
use std::sync::Mutex;

const VERSIONBITS_TOP_BITS: u32 = 0x2000_0000;
const VERSIONBITS_TOP_MASK: u32 = 0xe000_0000;
const VERSIONBITS_NUM_BITS: i32 = 29;

/// Period / threshold for unknown-bit warnings.
pub fn warn_period_threshold(network: Network) -> (u32, u32) {
    match network {
        Network::Mainnet => (2016, 1815),
        Network::Testnet | Network::Signet => (2016, 2016 * 3 / 4),
        Network::Regtest => (144, 144 * 3 / 4),
    }
}

/// Blocks below this height are not counted (Core `MinBIP9WarningHeight`).
pub fn warn_min_height(network: Network) -> u32 {
    match network {
        Network::Mainnet => 711_648,
        Network::Testnet => 2_013_984,
        Network::Signet | Network::Regtest => 0,
    }
}

/// Format Core's unknown-rules warning for `bit`.
pub fn unknown_rules_warning(bit: i32) -> String {
    format!("Unknown new rules activated (versionbit {bit})")
}

/// Unknown bits that reached ACTIVE on the best chain, kept across calls.
///
/// A bit locked in during period `p` is ACTIVE from period `p + 2`, and
/// ACTIVE is final, so each completed period is read once and only periods
/// completed since the last call are read. RAM is a few words; the trade is
/// one header pass per new period instead of 29 passes from genesis per RPC
/// call (≈4.7 s per `getnetworkinfo` at mainnet height 968k).
pub(crate) struct UnknownBitsScan {
    /// Periods `0..scanned_periods` are counted.
    scanned_periods: u32,
    /// Hash of the last header of the last counted period (reorg check).
    boundary: [u8; 32],
    /// Bits that reached the threshold in a counted period.
    locked: u32,
}

impl UnknownBitsScan {
    const fn new() -> Self {
        Self {
            scanned_periods: 0,
            boundary: [0; 32],
            locked: 0,
        }
    }

    pub(crate) fn active_bits(&mut self, query: &Query, network: Network) -> Vec<i32> {
        let Some(tip) = query.tip_height() else {
            return Vec::new();
        };
        let (period, threshold) = warn_period_threshold(network);
        self.advance(query, tip.0, period, threshold, warn_min_height(network))
    }

    fn advance(
        &mut self,
        query: &Query,
        tip: u32,
        period: u32,
        threshold: u32,
        min_height: u32,
    ) -> Vec<i32> {
        // The most recent completed period is at most LOCKED_IN, not ACTIVE.
        let target = (tip / period).saturating_sub(1);
        if target < self.scanned_periods || !self.boundary_matches(query, period) {
            self.scanned_periods = 0;
            self.boundary = [0; 32];
            self.locked = 0;
        }
        for p in self.scanned_periods..target {
            let mut counts = [0u32; VERSIONBITS_NUM_BITS as usize];
            for h in (p * period).max(min_height).max(1)..(p + 1) * period {
                if let Ok(Some((_, rec))) = query.header_at_height(Height(h)) {
                    for (bit, count) in counts.iter_mut().enumerate() {
                        if signals_unknown(&rec.version, bit as i32) {
                            *count += 1;
                        }
                    }
                }
            }
            for (bit, count) in counts.iter().enumerate() {
                if *count >= threshold {
                    self.locked |= 1 << bit;
                }
            }
        }
        if target > self.scanned_periods {
            self.scanned_periods = target;
            self.boundary = query
                .header_at_height(Height(period_boundary_height(target, period)))
                .ok()
                .flatten()
                .map_or([0; 32], |(_, rec)| rec.hash);
        }
        (0..VERSIONBITS_NUM_BITS)
            .filter(|bit| (self.locked >> bit) & 1 == 1)
            .collect()
    }

    fn boundary_matches(&self, query: &Query, period: u32) -> bool {
        if self.scanned_periods == 0 {
            return true;
        }
        let h = period_boundary_height(self.scanned_periods, period);
        matches!(query.header_at_height(Height(h)), Ok(Some((_, rec))) if rec.hash == self.boundary)
    }
}

/// Last header of period index `periods` (the block at `periods * period - 1`).
fn period_boundary_height(periods: u32, period: u32) -> u32 {
    periods * period - 1
}

fn signals_unknown(version: &i32, bit: i32) -> bool {
    let v = *version as u32;
    (v & VERSIONBITS_TOP_MASK) == VERSIONBITS_TOP_BITS && ((v >> bit) & 1) != 0
}

/// Warning strings for RPC `warnings` arrays.
pub fn warning_strings(query: &Query, network: Network) -> Vec<String> {
    static SCAN: Mutex<UnknownBitsScan> = Mutex::new(UnknownBitsScan::new());
    SCAN.lock()
        .unwrap_or_else(|e| e.into_inner())
        .active_bits(query, network)
        .into_iter()
        .map(unknown_rules_warning)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Core `WarningBitsConditionChecker`: mainnet keeps 1815 of 2016 (BIP
    /// 341's 90%), test chains take 75% of the difficulty interval, and each
    /// chain counts from its `MinBIP9WarningHeight`.
    #[test]
    fn warn_rule_matches_core_chainparams() {
        let rule = |n| (warn_period_threshold(n), warn_min_height(n));
        assert_eq!(rule(Network::Mainnet), ((2016, 1815), 711_648));
        assert_eq!(rule(Network::Testnet), ((2016, 1512), 2_013_984));
        assert_eq!(rule(Network::Signet), ((2016, 1512), 0));
        assert_eq!(rule(Network::Regtest), ((144, 108), 0));
    }

    #[test]
    fn warning_text_matches_core() {
        assert_eq!(
            unknown_rules_warning(27),
            "Unknown new rules activated (versionbit 27)"
        );
    }

    #[test]
    fn top_bits_signal_detection() {
        let v = (VERSIONBITS_TOP_BITS | (1 << 27)) as i32;
        assert!(signals_unknown(&v, 27));
        assert!(!signals_unknown(&v, 26));
        assert!(!signals_unknown(&(VERSIONBITS_TOP_BITS as i32), 27));
    }

    /// Regtest-shaped chain of `n` headers; heights in `signal` set bit 27.
    fn put_chain(q: &Query, n: u32, signal: impl Fn(u32) -> bool, salt: u8) {
        use rbitcoin_primitives::Fk;
        use rbitcoin_store::HeaderRecord;
        let mut prev_fk = Fk::NULL;
        let mut prev_hash = [0u8; 32];
        for h in 0u32..n {
            let mut merkle = [salt; 32];
            merkle[0..4].copy_from_slice(&h.to_le_bytes());
            let version = if h > 0 && signal(h) {
                (VERSIONBITS_TOP_BITS | (1 << 27)) as i32
            } else {
                1
            };
            let hash = if h == 0 {
                merkle
            } else {
                rbitcoin_store::block_header_hash(
                    version,
                    &prev_hash,
                    &merkle,
                    h + 1,
                    0x207fffff,
                    h,
                )
            };
            let rec = HeaderRecord {
                prev_fk,
                version,
                timestamp: h + 1,
                bits: 0x207fffff,
                nonce: h,
                merkle_root: merkle,
                hash,
                size: 0,
                weight: 0,
            };
            prev_fk = q.put_header(&rec).unwrap();
            q.store().confirmed.set(Height(h), prev_fk).unwrap();
            prev_hash = hash;
        }
        q.store().rebuild_height_fence().unwrap();
    }

    /// Activation needs a signalling period plus one more; only blocks at or
    /// above the floor count (so mainnet's CSV, SegWit and Taproot periods
    /// raise no warning); the scan keeps its place across calls
    /// (`getnetworkinfo` used to re-read every header per bit, ≈4.7 s on
    /// mainnet) and starts over when the boundary changes.
    #[test]
    fn unknown_bit_scan_activates_and_keeps_its_place() {
        let (dir, q) = rbitcoin_query::testutil::tiny_query_labeled("vb-active");
        assert!(UnknownBitsScan::new().advance(&q, 0, 4, 2, 0).is_empty());
        assert!(UnknownBitsScan::new().advance(&q, 4, 4, 2, 0).is_empty());

        put_chain(&q, 13, |h| h < 4, 0);
        let mut scan = UnknownBitsScan::new();
        assert_eq!(scan.advance(&q, 8, 4, 2, 0), vec![27]);
        assert_eq!(scan.scanned_periods, 1);
        let (_, boundary) = q.header_at_height(Height(3)).unwrap().unwrap();
        assert_eq!(scan.boundary, boundary.hash, "counted boundary is height 3");
        scan.locked = 0;
        assert!(
            scan.advance(&q, 8, 4, 2, 0).is_empty(),
            "same tip does not reread a finished period"
        );
        assert_eq!(scan.scanned_periods, 1, "same tip: nothing new to read");
        assert!(
            scan.advance(&q, 12, 4, 2, 0).is_empty(),
            "a later period that does not signal stays locked-empty"
        );
        assert_eq!(scan.scanned_periods, 2);
        assert!(UnknownBitsScan::new().advance(&q, 8, 4, 4, 0).is_empty());
        assert_eq!(UnknownBitsScan::new().advance(&q, 8, 4, 2, 2), vec![27]);
        assert!(UnknownBitsScan::new().advance(&q, 8, 4, 2, 3).is_empty());

        // Period 1 signals only at its last two blocks. Threshold 3 stays
        // empty; threshold 2 locks. A wider or shorter window would disagree.
        let (dir3, wide) = rbitcoin_query::testutil::tiny_query_labeled("vb-wide");
        put_chain(&wide, 13, |h| h < 4 || h == 6 || h == 7, 2);
        let (_, wide_boundary) = wide.header_at_height(Height(3)).unwrap().unwrap();
        let mut under = UnknownBitsScan::new();
        under.scanned_periods = 1;
        under.boundary = wide_boundary.hash;
        assert!(
            under.advance(&wide, 12, 4, 3, 0).is_empty(),
            "two signals in period 1 are under threshold 3"
        );
        let mut over = UnknownBitsScan::new();
        over.scanned_periods = 1;
        over.boundary = wide_boundary.hash;
        assert_eq!(over.advance(&wide, 12, 4, 2, 0), vec![27]);

        let (dir2, other) = rbitcoin_query::testutil::tiny_query_labeled("vb-other");
        put_chain(&other, 9, |_| false, 1);
        assert!(
            scan.advance(&other, 8, 4, 2, 0).is_empty(),
            "a different header at the counted boundary starts over"
        );
        assert_eq!(scan.scanned_periods, 1);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
        let _ = std::fs::remove_dir_all(&dir3);
    }
}
