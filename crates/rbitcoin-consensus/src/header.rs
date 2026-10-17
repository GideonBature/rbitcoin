use crate::error::ConsensusError;
use crate::params::{check_genesis_hash, ChainParams};
use bitcoin::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::{CompactTarget, Target};
use rbitcoin_primitives::Height;
use rbitcoin_query::Query;

/// Parent header facts for [`check_header`]. Copied so a height above zero can
/// use the same parent for linkage and for the work calculation.
#[derive(Clone, Copy)]
pub struct ParentHeader {
    pub hash: [u8; 32],
    pub time: u32,
    pub bits: CompactTarget,
}

/// Header rules from the header bytes plus caller facts. No datadir.
///
/// `mtp_times` ends at the parent (up to 11 timestamps). `period_first_time`
/// is the header `interval` blocks before a retarget height. `bits_at` supplies
/// earlier `nBits` for the testnet min-difficulty walk.
pub fn check_header(
    params: &ChainParams,
    height: Height,
    header: &Header,
    hash: [u8; 32],
    parent: Option<ParentHeader>,
    mtp_times: &[u32],
    period_first_time: Option<u32>,
    mut bits_at: impl FnMut(u32) -> Option<CompactTarget>,
) -> Result<(), ConsensusError> {
    let hash_bh = bitcoin::BlockHash::from_byte_array(hash);
    if height.0 == 0 {
        if !check_genesis_hash(params, hash_bh) {
            return Err(ConsensusError::BadHeader("genesis hash mismatch"));
        }
        let expected = crate::params::genesis_block(params).header.bits;
        if header.bits != expected {
            return Err(ConsensusError::BadHeader("incorrect proof of work bits"));
        }
        return pow_hash_meets_target(hash, header.bits, params.pow_limit);
    }

    let parent = parent.ok_or(ConsensusError::BadPrev)?;
    if parent.hash != header.prev_blockhash.to_byte_array() {
        return Err(ConsensusError::BadPrev);
    }
    let mtp = median_time_past_times(mtp_times)
        .map_err(|_| ConsensusError::BadHeader("empty median time"))?;
    if header.time <= mtp {
        return Err(ConsensusError::BadHeader("timestamp <= median-time-past"));
    }
    check_header_version_and_future_time(params, height, header)?;

    let interval = params.difficulty_adjustment_interval();
    if interval > 0
        && height.0.is_multiple_of(interval)
        && !params.no_pow_retargeting()
        && period_first_time.is_none()
    {
        return Err(ConsensusError::BadHeader("missing retarget first header"));
    }
    let expected = next_work_bits(
        params,
        height.0,
        parent.bits,
        parent.time,
        header.time,
        period_first_time,
        &mut bits_at,
    )
    .ok_or(ConsensusError::BadPrev)?;
    if header.bits != expected {
        return Err(ConsensusError::BadHeader("incorrect proof of work bits"));
    }
    pow_hash_meets_target(hash, header.bits, params.pow_limit)
}

/// Validate header linkage, MTP, difficulty bits, and proof-of-work.
pub fn validate_header(
    query: &Query,
    params: &ChainParams,
    height: Height,
    header: &Header,
) -> Result<(), ConsensusError> {
    validate_header_hashed(
        query,
        params,
        height,
        header,
        header.block_hash().to_byte_array(),
    )
}

/// [`validate_header`] using a caller-computed header hash (lookup already hashed).
pub(crate) fn validate_header_hashed(
    query: &Query,
    params: &ChainParams,
    height: Height,
    header: &Header,
    hash: [u8; 32],
) -> Result<(), ConsensusError> {
    let parent = if height.0 == 0 {
        None
    } else {
        query
            .header_at_height(Height(height.0 - 1))?
            .map(|(_fk, prev)| ParentHeader {
                hash: prev.hash,
                time: prev.timestamp,
                bits: CompactTarget::from_consensus(prev.bits),
            })
    };
    let times = if parent.is_some() {
        mtp_times_ending_at(query, Height(height.0 - 1))?
    } else {
        Vec::new()
    };
    let period_first = if parent.is_some() {
        period_first_time(query, params, height.0)?
    } else {
        None
    };
    check_header(
        params,
        height,
        header,
        hash,
        parent,
        &times,
        period_first,
        |h| {
            header_bits_at(query, Height(h))
                .ok()
                .map(CompactTarget::from_consensus)
        },
    )
}

/// Contextual checks when the parent is a known header, not necessarily on the
/// best chain (`header_at_height` would miss).
pub fn validate_header_on_parent(
    params: &ChainParams,
    height: Height,
    header: &Header,
    parent_mtp: u32,
    expected_bits: CompactTarget,
) -> Result<(), ConsensusError> {
    if header.time <= parent_mtp {
        return Err(ConsensusError::BadHeader("timestamp <= median-time-past"));
    }
    check_header_version_and_future_time(params, height, header)?;
    if header.bits != expected_bits {
        return Err(ConsensusError::BadHeader("incorrect proof of work bits"));
    }
    pow_hash_meets_target(
        header.block_hash().to_byte_array(),
        header.bits,
        params.pow_limit,
    )
}

/// POW vs a **caller-computed** header hash (no second SHA256d).
pub(crate) fn pow_hash_meets_target(
    hash: [u8; 32],
    bits: CompactTarget,
    pow_limit: Target,
) -> Result<(), ConsensusError> {
    let target = Target::from_compact(bits);
    if target > pow_limit {
        return Err(ConsensusError::BadHeader("target above pow limit"));
    }
    if !target.is_met_by(bitcoin::BlockHash::from_byte_array(hash)) {
        return Err(ConsensusError::InvalidPow);
    }
    Ok(())
}

/// BIP34/66/65 `nVersion` floors and the 2-hour future-time cap (wall clock).
///
/// Core `ContextualCheckBlockHeader`. Assemble uses this instead of a second
/// [`validate_header`] (MTP walk + header rehash).
pub(crate) fn check_header_version_and_future_time(
    params: &ChainParams,
    height: Height,
    header: &Header,
) -> Result<(), ConsensusError> {
    const MAX_FUTURE_BLOCK_TIME: u64 = 2 * 60 * 60;
    let now = crate::clock::current_now();
    if u64::from(header.time) > now.saturating_add(MAX_FUTURE_BLOCK_TIME) {
        return Err(ConsensusError::BadHeader("timestamp too far in future"));
    }
    let ver = header.version.to_consensus();
    if (params.bip34_active_at(height.0) && ver < 2)
        || (params.bip66_active_at(height.0) && ver < 3)
        || (params.bip65_active_at(height.0) && ver < 4)
    {
        return Err(ConsensusError::BadVersion(ver));
    }
    Ok(())
}

/// Median timestamp of up to 11 blocks ending at `height` (inclusive).
///
/// **Confirm load path:** BIP68/BIP113 run during multi-block assemble while
/// mid-batch heights are not yet in `confirmed[]`. Prefer the durable confirmed
/// chain, then load-stage header plans (same hybrid as [`crate::confirm_run`]
/// header MTP). Heights still above tip with no plan are retryable load
/// incomplete — not permanent `BadPrev` (that silently split batches to n=1).
pub fn median_time_past(query: &Query, height: Height) -> Result<u32, ConsensusError> {
    let times = mtp_times_ending_at(query, height)?;
    median_time_past_times(&times).map_err(|_| ConsensusError::BadHeader("empty median time"))
}

/// Timestamps ending at `height` for [`median_time_past`]. A missing plan above
/// the confirmed tip stays a retryable load fault. A gap at or below the tip
/// is [`ConsensusError::BadPrev`].
fn mtp_times_ending_at(query: &Query, height: Height) -> Result<Vec<u32>, ConsensusError> {
    if let Some((n, buf)) = query.store().mtp_times_at(height) {
        return Ok(buf[..n as usize].to_vec());
    }
    let mut times = Vec::with_capacity(11);
    let start = height.0.saturating_sub(10);
    let tip = query.tip_height().map(|h| h.0).unwrap_or(0);
    for h in start..=height.0 {
        if let Some((_fk, rec)) = query.header_at_height(Height(h))? {
            times.push(rec.timestamp);
            continue;
        }
        if let Some(plan) = query.confirm_parent_cache().get_header_plan(h) {
            times.push(plan.header_rec.timestamp);
            continue;
        }
        if h > tip {
            return Err(ConsensusError::Store(rbitcoin_store::StoreError::Corrupt(
                "confirm: load incomplete (parent header plan missing above tip)",
            )));
        }
        return Err(ConsensusError::BadPrev);
    }
    Ok(times)
}

/// MTP from the confirmed chain only (write structural). Tip-ahead heights
/// must be carried on [`crate::confirm_run`] `Prepared::prev_mtp`.
pub fn median_time_past_store(query: &Query, height: Height) -> Result<u32, ConsensusError> {
    if let Some((n, buf)) = query.store().mtp_times_at(height) {
        return median_time_past_times(&buf[..n as usize])
            .map_err(|_| ConsensusError::BadHeader("empty median time"));
    }
    let mut times = Vec::with_capacity(11);
    let start = height.0.saturating_sub(10);
    for h in start..=height.0 {
        if let Some((_fk, rec)) = query.header_at_height(Height(h))? {
            times.push(rec.timestamp);
            continue;
        }
        return Err(ConsensusError::Store(rbitcoin_store::StoreError::Corrupt(
            "confirm: write MTP missing confirmed header (carry prev_mtp)",
        )));
    }
    median_time_past_times(&times).map_err(|_| ConsensusError::BadHeader("empty median time"))
}

pub use rbitcoin_primitives::median_time_past_times;

/// Expected `nBits` for a new header at `height`.
///
/// `header_time` is the candidate block's timestamp (Core `pblock->GetBlockTime()`).
pub fn expected_next_bits(
    query: &Query,
    params: &ChainParams,
    height: Height,
    header_time: u32,
) -> Result<CompactTarget, ConsensusError> {
    if height.0 == 0 {
        let g = crate::params::genesis_block(params);
        return Ok(g.header.bits);
    }

    let prev_height = Height(height.0 - 1);
    let (_fk, prev_rec) = query
        .header_at_height(prev_height)?
        .ok_or(ConsensusError::BadPrev)?;
    let prev_bits = CompactTarget::from_consensus(prev_rec.bits);
    let period_first = period_first_time(query, params, height.0)?;
    next_work_bits(
        params,
        height.0,
        prev_bits,
        prev_rec.timestamp,
        header_time,
        period_first,
        |h| {
            header_bits_at(query, Height(h))
                .ok()
                .map(CompactTarget::from_consensus)
        },
    )
    .ok_or(ConsensusError::BadPrev)
}

/// Difficulty for the header at `height`, from the parent and the period start.
///
/// `period_first_time` is the timestamp of the header at `height - interval`
/// when `height` is a retarget boundary. `bits_at` supplies earlier `nBits`
/// for the testnet min-difficulty walk. `None` means an ancestor is missing.
pub fn next_work_bits(
    params: &ChainParams,
    height: u32,
    prev_bits: CompactTarget,
    prev_time: u32,
    header_time: u32,
    period_first_time: Option<u32>,
    mut bits_at: impl FnMut(u32) -> Option<CompactTarget>,
) -> Option<CompactTarget> {
    if height == 0 {
        return None;
    }
    let interval = params.difficulty_adjustment_interval();
    if interval == 0 || !height.is_multiple_of(interval) {
        return min_diff_bits(
            params,
            height,
            prev_bits,
            prev_time,
            header_time,
            &mut bits_at,
        );
    }
    if params.no_pow_retargeting() {
        return Some(prev_bits);
    }
    let first_time = period_first_time?;
    let timespan = u64::from(prev_time.saturating_sub(first_time));
    Some(CompactTarget::from_next_work_required(
        prev_bits,
        timespan,
        &params.btc,
    ))
}

fn period_first_time(
    query: &Query,
    params: &ChainParams,
    height: u32,
) -> Result<Option<u32>, ConsensusError> {
    let interval = params.difficulty_adjustment_interval();
    if interval == 0 || !height.is_multiple_of(interval) || params.no_pow_retargeting() {
        return Ok(None);
    }
    Ok(query
        .header_at_height(Height(height - interval))?
        .map(|(_fk, first_rec)| first_rec.timestamp))
}

fn min_diff_bits(
    params: &ChainParams,
    height: u32,
    prev_bits: CompactTarget,
    prev_time: u32,
    header_time: u32,
    bits_at: &mut impl FnMut(u32) -> Option<CompactTarget>,
) -> Option<CompactTarget> {
    if !params.allow_min_difficulty_blocks() {
        return Some(prev_bits);
    }
    let limit = params.pow_limit.to_compact_lossy();
    let spacing = params.btc.pow_target_spacing;
    if u64::from(header_time) > u64::from(prev_time).saturating_add(spacing.saturating_mul(2)) {
        return Some(limit);
    }
    let interval = params.difficulty_adjustment_interval();
    let mut h = height - 1;
    let mut bits = prev_bits;
    while interval > 0 && !h.is_multiple_of(interval) && bits == limit {
        if h == 0 {
            break;
        }
        h -= 1;
        bits = bits_at(h)?;
    }
    Some(bits)
}

/// Compact `nBits` as a difficulty relative to the max target (Core `GetDifficulty`).
pub fn difficulty_from_bits(bits: u32) -> f64 {
    let n_shift = ((bits >> 24) & 0xff) as i32;
    let mut ddiff = (0x0000_ffff_u64 as f64) / ((bits & 0x00ff_ffff) as f64);
    let mut shift = n_shift - 29;
    while shift < 0 {
        ddiff *= 256.0;
        shift += 1;
    }
    while shift > 0 {
        ddiff /= 256.0;
        shift -= 1;
    }
    ddiff
}

fn header_bits_at(query: &Query, height: Height) -> Result<u32, ConsensusError> {
    if let Some((_fk, rec)) = query.header_at_height(height)? {
        return Ok(rec.bits);
    }
    if let Some(plan) = query.confirm_parent_cache().get_header_plan(height.0) {
        return Ok(plan.header_rec.bits);
    }
    Err(ConsensusError::BadPrev)
}

#[cfg(test)]
mod median_time_past_tests {
    use super::*;
    use bitcoin::block::Version;
    use rbitcoin_primitives::{Fk, Height};
    use rbitcoin_query::testutil::FixtureChain;
    use rbitcoin_query::{Query, TxApply};
    use rbitcoin_store::{HeaderRecord, InputRecord, OutputRecord, TxRecord};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn max_target_bits_are_difficulty_one() {
        assert_eq!(difficulty_from_bits(0x1d00_ffff), 1.0);
    }

    #[test]
    fn pow_hash_meets_target_meet_miss_and_limit() {
        let rt = ChainParams::regtest();
        let easy = CompactTarget::from_consensus(0x207f_ffff);
        pow_hash_meets_target([0u8; 32], easy, rt.pow_limit).unwrap();
        let mainnet_bits = CompactTarget::from_consensus(0x1d00_ffff);
        let miss = pow_hash_meets_target([0xff; 32], mainnet_bits, Target::MAX_ATTAINABLE_MAINNET)
            .unwrap_err();
        assert!(matches!(miss, ConsensusError::InvalidPow), "{miss:?}");
        let err =
            pow_hash_meets_target([0u8; 32], easy, ChainParams::mainnet().pow_limit).unwrap_err();
        assert!(
            matches!(err, ConsensusError::BadHeader(s) if s.contains("pow limit")),
            "{err:?}"
        );
    }

    /// Mainnet block 32256 (`000000004f2886a170adb7204cb0c7a824217dd24d11a74423d564c4e0904967`).
    ///
    /// Parent 32255 time `1262152739`, bits `0x1d00ffff`. Period start is
    /// height 30240, time `1261130161`. Timespan `1022578` is inside the
    /// clamp, so the compact is Core's `GetCompact` of that product:
    /// `0x1d00d86a`. The literal is not produced by calling the function
    /// under test.
    #[test]
    fn mainnet_retarget_32256_matches_core_compact() {
        let params = ChainParams::mainnet();
        let got = next_work_bits(
            &params,
            32_256,
            CompactTarget::from_consensus(0x1d00_ffff),
            1_262_152_739,
            1_262_152_739,
            Some(1_261_130_161),
            |_| None,
        );
        assert_eq!(got, Some(CompactTarget::from_consensus(0x1d00_d86a)));
    }

    /// Clamp and pow-limit cap, with expected compacts written as integers.
    ///
    /// Core order: timespan clamped to `[target/4, target*4]` (1_209_600 is
    /// two weeks), `target * timespan / 1_209_600`, then cap at the pow
    /// limit, then `GetCompact`.
    ///
    /// - Quarter floor from `0x1d00ffff` and a 1-second span. Clamped span is
    ///   302_400. `0xffff << 208` * 302_400 / 1_209_600 compacts to
    ///   `0x1c3fffc0`. Without the floor the compact is `0x1a0ddead`.
    /// - 4× ceiling from `0x1c1fffe0` (`0xffff << 205`) and a span one second
    ///   past four periods. Clamped product compacts to `0x1c7fff80`, still
    ///   under the pow limit. Without the ceiling the compact is `0x1c7fff81`.
    /// - Pow-limit cap from `0x1d007fff` (`0x7fff << 208`) and an exact 4×
    ///   span. The product exceeds the limit. Compact is `0x1d00ffff`.
    ///   Without the cap it is `0x1d01fffc`.
    #[test]
    fn retarget_clamps_timespan_and_caps_at_pow_limit() {
        let params = ChainParams::mainnet();
        let bits = |prev: u32, prev_time: u32, first: u32| {
            next_work_bits(
                &params,
                2_016,
                CompactTarget::from_consensus(prev),
                prev_time,
                prev_time,
                Some(first),
                |_| None,
            )
        };
        assert_eq!(
            bits(0x1d00_ffff, 2, 1),
            Some(CompactTarget::from_consensus(0x1c3f_ffc0))
        );
        assert_eq!(
            bits(0x1c1f_ffe0, 4_838_402, 1),
            Some(CompactTarget::from_consensus(0x1c7f_ff80))
        );
        assert_eq!(
            bits(0x1d00_7fff, 4_838_401, 1),
            Some(CompactTarget::from_consensus(0x1d00_ffff))
        );
    }

    #[test]
    fn mtp_times_picks_middle_of_sorted() {
        assert_eq!(median_time_past_times(&[3, 1, 2]).unwrap(), 2);
        assert_eq!(median_time_past_times(&[10]).unwrap(), 10);
        // Even length: Core takes sorted[len/2] (upper middle).
        assert_eq!(median_time_past_times(&[1, 2, 3, 4]).unwrap(), 3);
    }

    fn temp_q() -> (std::path::PathBuf, Query) {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbitcoin-hdr-mtp-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let q = Query::open_or_create_tiny(dir.join("store")).unwrap();
        (dir, q)
    }

    /// Write-gate-safe synthetic coinbase: non-null `prev_fk` must commit `parent_hash`.
    fn coinbase(h: u32, prev: Fk, parent_hash: Option<[u8; 32]>) -> (HeaderRecord, TxApply) {
        let version = 1;
        let timestamp = 1_000 + h * 10;
        let bits = 0x207fffff;
        let nonce = h;
        let mut merkle = [0u8; 32];
        merkle[0..4].copy_from_slice(&h.to_le_bytes());
        merkle[4] = 0xcd;
        let hash = match parent_hash {
            None => merkle,
            Some(ph) => {
                rbitcoin_store::block_header_hash(version, &ph, &merkle, timestamp, bits, nonce)
            }
        };
        let header = HeaderRecord {
            prev_fk: prev,
            version,
            timestamp,
            bits,
            nonce,
            merkle_root: merkle,
            hash,
            size: 0,
            weight: 0,
        };
        let mut txid = [0u8; 32];
        txid[0..4].copy_from_slice(&h.to_le_bytes());
        txid[31] = 0xcb;
        let ta = TxApply {
            tx: TxRecord {
                txid,
                version: 1,
                locktime: 0,
                input_start_fk: Fk::NULL,
                input_count: 1,
                output_start_fk: Fk::NULL,
                output_count: 1,
            },
            inputs: vec![InputRecord {
                prev_txid: [0u8; 32],
                parent: rbitcoin_store::InputParent::Coinbase,
                sequence: u32::MAX,
                script_sig: vec![h as u8],
                witness: vec![],
            }],
            outputs: vec![OutputRecord::unspent(50, vec![0x51])],
        };
        (header, ta)
    }

    #[test]
    fn mtp_from_confirmed_chain_and_missing_above_tip() {
        let (dir, q) = temp_q();
        let mut prev = Fk::NULL;
        let mut parent_hash: Option<[u8; 32]> = None;
        for h in 0..3u32 {
            let (hdr, ta) = coinbase(h, prev, parent_hash);
            parent_hash = Some(hdr.hash);
            prev = q.connect_block(Height(h), &hdr, &[ta]).unwrap();
        }
        let mtp = median_time_past(&q, Height(2)).unwrap();
        // times: 1000, 1010, 1020 → middle 1010
        assert_eq!(mtp, 1010);
        let (n, buf) = q
            .store()
            .mtp_times_at(Height(2))
            .expect("ring covers confirmed tip");
        assert_eq!(n, 3);
        assert_eq!(median_time_past_times(&buf[..3]).unwrap(), 1010);

        // Height above tip with no plan → incomplete load error (not BadPrev).
        let err = median_time_past(&q, Height(5)).unwrap_err();
        assert!(
            format!("{err}").contains("load incomplete"),
            "above tip must be the retryable load error, got {err:?}"
        );

        let (empty_dir, empty) = temp_q();
        let err = median_time_past(&empty, Height(0)).unwrap_err();
        assert!(
            matches!(err, ConsensusError::BadPrev),
            "a missing header at the tip is BadPrev, got {err:?}"
        );
        let _ = std::fs::remove_dir_all(&empty_dir);

        // expected_next_bits: height 0 + regtest no-retarget.
        let params = ChainParams::regtest();
        let gbits = expected_next_bits(&q, &params, Height(0), 0).unwrap();
        assert_eq!(gbits, crate::params::genesis_block(&params).header.bits);
        let b1 = expected_next_bits(&q, &params, Height(1), 0).unwrap();
        let (_fk, rec0) = q.header_at_height(Height(0)).unwrap().unwrap();
        assert_eq!(b1.to_consensus(), rec0.bits);

        // Bad prev header height.
        assert!(expected_next_bits(&q, &params, Height(99), 0).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sequential_tip_mtp_matches_ring_and_survives_pop() {
        let (dir, q) = temp_q();
        let mut prev = Fk::NULL;
        let mut parent_hash: Option<[u8; 32]> = None;
        let mut times = Vec::new();
        for h in 0..12u32 {
            let (hdr, ta) = coinbase(h, prev, parent_hash);
            times.push(hdr.timestamp);
            parent_hash = Some(hdr.hash);
            prev = q.connect_block(Height(h), &hdr, &[ta]).unwrap();
        }
        let want11 = median_time_past_times(&times[1..]).unwrap();
        assert_eq!(median_time_past_store(&q, Height(11)).unwrap(), want11);
        assert_eq!(median_time_past(&q, Height(11)).unwrap(), want11);
        let (n, buf) = q.store().mtp_times_at(Height(11)).expect("ring at tip");
        assert_eq!(n, 11);
        assert_eq!(median_time_past_times(&buf[..11]).unwrap(), want11);
        assert!(
            q.store().mtp_times_at(Height(5)).is_none(),
            "historical MTP is not the tip ring"
        );
        assert_eq!(
            median_time_past_store(&q, Height(5)).unwrap(),
            median_time_past_times(&times[0..=5]).unwrap()
        );

        q.disconnect_tip().unwrap();
        let want10 = median_time_past_times(&times[0..=10]).unwrap();
        assert_eq!(median_time_past_store(&q, Height(10)).unwrap(), want10);
        let (n, buf) = q
            .store()
            .mtp_times_at(Height(10))
            .expect("ring rebuilt after pop");
        assert_eq!(median_time_past_times(&buf[..n as usize]).unwrap(), want10);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_header_genesis_and_bad_prev() {
        let (dir, q) = temp_q();
        let params = ChainParams::regtest();
        let g = crate::params::genesis_block(&params);
        // Wrong genesis hash at height 0.
        let mut bad = g.header;
        bad.nonce ^= 1;
        let err = validate_header(&q, &params, Height(0), &bad).unwrap_err();
        assert!(matches!(err, ConsensusError::BadHeader(_)), "{err:?}");

        // Synthetic tip for prev linkage (child hash commits to parent).
        let (h0, ta0) = coinbase(0, Fk::NULL, None);
        let prev = q.connect_block(Height(0), &h0, &[ta0]).unwrap();
        let (h1, ta1) = coinbase(1, prev, Some(h0.hash));
        q.connect_block(Height(1), &h1, &[ta1]).unwrap();

        // Header with wrong prev hash at height 1.
        let mut hdr = g.header;
        hdr.prev_blockhash = bitcoin::BlockHash::from_byte_array([0xee; 32]);
        hdr.time = 2_000;
        hdr.bits = bitcoin::CompactTarget::from_consensus(0x207fffff);
        let err = validate_header(&q, &params, Height(1), &hdr).unwrap_err();
        assert!(
            matches!(
                err,
                ConsensusError::BadPrev
                    | ConsensusError::BadHeader(_)
                    | ConsensusError::BadVersion(_)
            ),
            "{err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn testnet_min_difficulty_after_20_minute_gap() {
        let (dir, q) = temp_q();
        let params = ChainParams::testnet();
        assert!(params.allow_min_difficulty_blocks());
        let (h0, ta0) = coinbase(0, Fk::NULL, None);
        let prev_fk = q.connect_block(Height(0), &h0, &[ta0]).unwrap();
        let limit = params.pow_limit.to_compact_lossy();
        assert_ne!(CompactTarget::from_consensus(h0.bits), limit);

        let spacing = params.btc.pow_target_spacing as u32;
        let gap =
            expected_next_bits(&q, &params, Height(1), h0.timestamp + 2 * spacing + 1).unwrap();
        assert_eq!(gap, limit);
        let eq_boundary =
            expected_next_bits(&q, &params, Height(1), h0.timestamp + 2 * spacing).unwrap();
        assert_eq!(eq_boundary.to_consensus(), h0.bits);

        let (mut h1, ta1) = coinbase(1, prev_fk, Some(h0.hash));
        h1.timestamp = h0.timestamp + 2 * spacing + 1;
        h1.bits = limit.to_consensus();
        h1.hash = rbitcoin_store::block_header_hash(
            h1.version,
            &h0.hash,
            &h1.merkle_root,
            h1.timestamp,
            h1.bits,
            h1.nonce,
        );
        q.connect_block(Height(1), &h1, &[ta1]).unwrap();
        let walked = expected_next_bits(&q, &params, Height(2), h1.timestamp + 100).unwrap();
        assert_eq!(walked.to_consensus(), h0.bits);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_work_bits_walks_back_past_min_difficulty() {
        let params = ChainParams::testnet();
        let limit = params.pow_limit.to_compact_lossy();
        let full = CompactTarget::from_consensus(0x1d00_eeee);
        assert_ne!(full, limit);
        let prev_time = 1_000_000u32;
        let walked = next_work_bits(
            &params,
            3,
            limit,
            prev_time,
            prev_time + 100,
            None,
            |h| match h {
                1 => Some(limit),
                0 => Some(full),
                _ => None,
            },
        )
        .unwrap();
        assert_eq!(walked, full);

        let spacing = params.btc.pow_target_spacing;
        let gap = next_work_bits(
            &params,
            3,
            full,
            prev_time,
            prev_time + (spacing as u32) * 2 + 1,
            None,
            |_| None,
        )
        .unwrap();
        assert_eq!(gap, limit);

        let mainnet = ChainParams::mainnet();
        let kept = next_work_bits(
            &mainnet,
            3,
            full,
            prev_time,
            prev_time + (spacing as u32) * 2 + 1,
            None,
            |_| Some(limit),
        )
        .unwrap();
        assert_eq!(kept, full);
    }

    #[test]
    fn check_header_rejects_without_a_store() {
        let params = ChainParams::mainnet();
        let genesis = crate::params::genesis_block(&params);
        let mut bad = genesis.header;
        bad.nonce = bad.nonce.wrapping_add(1);
        let err = check_header(
            &params,
            Height(0),
            &bad,
            bad.block_hash().to_byte_array(),
            None,
            &[],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(
            matches!(err, ConsensusError::BadHeader(s) if s.contains("genesis")),
            "{err:?}"
        );

        let parent = ParentHeader {
            hash: [1u8; 32],
            time: genesis.header.time,
            bits: genesis.header.bits,
        };
        let mut child = genesis.header;
        child.time = parent.time + 1;
        let missing = check_header(
            &params,
            Height(1),
            &child,
            [0xff; 32],
            None,
            &[parent.time],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(matches!(missing, ConsensusError::BadPrev), "{missing:?}");

        child.prev_blockhash = bitcoin::BlockHash::from_byte_array([2u8; 32]);
        let bad_prev = check_header(
            &params,
            Height(1),
            &child,
            [0xff; 32],
            Some(parent),
            &[parent.time],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(matches!(bad_prev, ConsensusError::BadPrev), "{bad_prev:?}");

        child.prev_blockhash = bitcoin::BlockHash::from_byte_array(parent.hash);
        child.time = parent.time;
        let mtp = check_header(
            &params,
            Height(1),
            &child,
            [0xff; 32],
            Some(parent),
            &[parent.time],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(
            matches!(mtp, ConsensusError::BadHeader(s) if s.contains("median-time-past")),
            "{mtp:?}"
        );

        child.time = parent.time + 1;
        child.bits = CompactTarget::from_consensus(0x1d00_eeee);
        let bits = check_header(
            &params,
            Height(1),
            &child,
            [0xff; 32],
            Some(parent),
            &[parent.time],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(
            matches!(bits, ConsensusError::BadHeader(s) if s.contains("proof of work bits")),
            "{bits:?}"
        );

        child.bits = parent.bits;
        let pow = check_header(
            &params,
            Height(1),
            &child,
            [0xff; 32],
            Some(parent),
            &[parent.time],
            None,
            |_| None,
        )
        .unwrap_err();
        assert!(matches!(pow, ConsensusError::InvalidPow), "{pow:?}");

        let interval = params.difficulty_adjustment_interval();
        let retarget = check_header(
            &params,
            Height(interval),
            &child,
            [0xff; 32],
            Some(parent),
            &[parent.time],
            None,
            |_| Some(parent.bits),
        )
        .unwrap_err();
        assert!(
            matches!(retarget, ConsensusError::BadHeader(s) if s.contains("missing retarget")),
            "{retarget:?}"
        );
    }

    #[test]
    fn check_header_version_and_future_time_regtest() {
        let params = ChainParams::regtest();
        let mut h = crate::params::genesis_block(&params).header;
        h.version = Version::from_consensus(1);
        crate::clock::with_now(1_700_000_000, || {
            h.time = 1_700_000_000;
            let err = check_header_version_and_future_time(&params, Height(1), &h).unwrap_err();
            assert!(matches!(err, ConsensusError::BadVersion(1)), "{err:?}");
            h.version = Version::from_consensus(4);
            h.time = 1_700_000_000 + 3 * 60 * 60;
            let err = check_header_version_and_future_time(&params, Height(1), &h).unwrap_err();
            assert!(
                matches!(err, ConsensusError::BadHeader(s) if s.contains("future")),
                "{err:?}"
            );
            h.time = 1_700_000_000 + 60 * 60;
            check_header_version_and_future_time(&params, Height(1), &h).unwrap();
        });
    }

    #[test]
    fn h8_timestamp_exactly_two_hours_accepts_plus_one_rejects() {
        let params = ChainParams::regtest();
        let mut h = crate::params::genesis_block(&params).header;
        h.version = Version::from_consensus(4);
        crate::clock::with_now(1_700_000_000, || {
            h.time = 1_700_000_000 + 2 * 60 * 60;
            check_header_version_and_future_time(&params, Height(1), &h).unwrap();
            h.time = 1_700_000_000 + 2 * 60 * 60 + 1;
            let err = check_header_version_and_future_time(&params, Height(1), &h).unwrap_err();
            assert!(
                matches!(err, ConsensusError::BadHeader(s) if s.contains("future")),
                "{err:?}"
            );
        });
    }

    #[test]
    fn h9_version_floors_at_bip34_66_65() {
        let rt = ChainParams::regtest();
        let mut h = crate::params::genesis_block(&rt).header;
        crate::clock::with_now(1_700_000_000, || {
            h.time = 1_700_000_000;
            h.version = Version::from_consensus(3);
            let err = check_header_version_and_future_time(&rt, Height(1), &h).unwrap_err();
            assert!(matches!(err, ConsensusError::BadVersion(3)), "{err:?}");
            h.version = Version::from_consensus(4);
            check_header_version_and_future_time(&rt, Height(1), &h).unwrap();
        });

        let main = ChainParams::mainnet();
        let mut mh = crate::params::genesis_block(&main).header;
        crate::clock::with_now(1_700_000_000, || {
            mh.time = 1_700_000_000;
            let bip34 = main.btc.bip34_height;
            mh.version = Version::from_consensus(1);
            check_header_version_and_future_time(&main, Height(bip34 - 1), &mh).unwrap();
            let err = check_header_version_and_future_time(&main, Height(bip34), &mh).unwrap_err();
            assert!(matches!(err, ConsensusError::BadVersion(1)), "{err:?}");
            mh.version = Version::from_consensus(2);
            check_header_version_and_future_time(&main, Height(bip34), &mh).unwrap();

            let bip66 = main.btc.bip66_height;
            mh.version = Version::from_consensus(2);
            let err = check_header_version_and_future_time(&main, Height(bip66), &mh).unwrap_err();
            assert!(matches!(err, ConsensusError::BadVersion(2)), "{err:?}");
            mh.version = Version::from_consensus(3);
            check_header_version_and_future_time(&main, Height(bip66), &mh).unwrap();

            let bip65 = main.btc.bip65_height;
            mh.version = Version::from_consensus(3);
            let err = check_header_version_and_future_time(&main, Height(bip65), &mh).unwrap_err();
            assert!(matches!(err, ConsensusError::BadVersion(3)), "{err:?}");
            mh.version = Version::from_consensus(4);
            check_header_version_and_future_time(&main, Height(bip65), &mh).unwrap();
        });
    }
}
