//! Work-chain progress snapshot, percent helpers, and rate/ETA for IBD logs.
//!
//! The operator `ibd: progress` line reports tip pace and in-RAM block-queue
//! occupancy. It does **not** print a retired dual-track Class A high-water or
//! "archive lead" (`arch_hwm` / `lead=`).

use super::body::BodyPresence;
use crate::chain::ChainHub;
use bitcoin::BlockHash;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cap status-tick hole walk so a huge missing header band cannot burn the loop.
const TIP_HOLE_SCAN_MAX: u32 = 8192;

/// Work-chain progress for status / progress logs.
///
/// - `tip`: confirmed best-chain height
/// - `ready_hwm`: highest claim-ready / offered body height on the work path
///   (body queue densify bookkeeping; **not** printed on the progress line)
/// - `headers`: max peer-advertised / learned header height
/// - `tip_hole`: count of heights from tip+1 until the next **claim-ready**
///   body (body queue / pending wire only) — the fetch gap operators care about
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkChainProgress {
    pub tip: u32,
    pub ready_hwm: u32,
    pub headers: u32,
    pub tip_hole: usize,
}

/// Inputs for the pure `ibd: progress` line formatter (unit-tested).
#[derive(Clone, Debug)]
pub(crate) struct ProgressLineInput {
    pub pct: u32,
    pub tip: u32,
    /// Tip advance rate over the last sample window (blocks/s).
    pub tip_rate: f64,
    pub tip_hole: usize,
    pub peers: usize,
    /// Confirm pipeline depths already formatted (`ready=… scriptq… writeq…`).
    pub conf_q: String,
    pub txs: u64,
    pub horizon: u32,
    /// From [`TipRateTracker::eta_string`] (`eta=…` or `done`).
    pub eta: String,
    /// In-RAM block queue bytes / entry count (process heap wire payloads).
    pub bq_bytes: u64,
    pub bq_count: usize,
    /// Soft densify confirm-window target (block count for ~1 min at tip rate).
    pub bq_soft_stop: u32,
}

/// Build the `ibd: progress …` message body (no log level prefix).
///
/// Tip percent/rate, **fetch hole** (tip→next claim-ready body), peers, confirm
/// queues, txs, header horizon, tip-rate ETA, in-RAM block-queue occupancy.
/// `bq soft=n/win` is count vs 1-min confirm window at tip rate; `RAM=` is queue heap MiB.
/// Count is only in `soft=` (no redundant `n=`).
pub(crate) fn format_progress_line(i: &ProgressLineInput) -> String {
    let bq_mib = i.bq_bytes / (1024 * 1024);
    format!(
        "ibd: progress {}% tip={} ({}/s) hole={} peers={} {} txs={} horizon={} {} bq soft={}/{} RAM={}MiB",
        i.pct,
        i.tip,
        format_rate(i.tip_rate),
        i.tip_hole,
        i.peers,
        i.conf_q,
        i.txs,
        i.horizon,
        i.eta,
        i.bq_count,
        i.bq_soft_stop,
        bq_mib,
    )
}

/// True if this height does not need another getdata.
///
/// In hand: confirmed, matching BQ hash, or already taken onto loadq
/// (`H ≤ lookup_taken_hi`). Class A alone is not enough. Pending without
/// BQ is not in hand (zombie). Rejected is never in hand.
pub(crate) fn claim_ready(
    hub: &ChainHub,
    body: &mut BodyPresence,
    height: u32,
    hash: &BlockHash,
) -> bool {
    use bitcoin::hashes::Hash as _;
    if hub.has_block(hash) {
        return true;
    }
    if body.is_rejected(hash) {
        return false;
    }
    if hub.query.lookup_already_taken(height) {
        return true;
    }
    // Must be **this** hash at the height — first-wins BQ of a different block
    // is not claim-ready (mainnet tip+1 wrong-wire thrash / permanent hole).
    hub.query
        .block_queue_hash_at_height(height)
        .is_some_and(|h| h == hash.to_byte_array())
}

/// Count heights from tip+1 until the next in-hand body (fetch gap).
///
/// - Missing header on the work path → stop (cannot request further).
/// - Rejected tip+1 → stop (not a download gap; confirm is blacklisted).
/// - In hand (confirmed / BQ hash / taken onto loadq) → stop.
/// - Otherwise increment hole (needs getdata).
#[cfg(test)]
pub(crate) fn tip_fetch_hole(
    hub: &ChainHub,
    height_to_hash: &HashMap<u32, BlockHash>,
    body: &mut BodyPresence,
) -> usize {
    tip_fetch_hole_at(hub, hub.tip_height(), height_to_hash, body)
}

pub(crate) fn tip_fetch_hole_at(
    hub: &ChainHub,
    tip: Option<u32>,
    height_to_hash: &HashMap<u32, BlockHash>,
    body: &mut BodyPresence,
) -> usize {
    let path_lo = match tip {
        None => 0u32,
        Some(t) => t.saturating_add(1),
    };
    let limit = path_lo.saturating_add(TIP_HOLE_SCAN_MAX);
    let mut flags = Vec::new();
    for ht in path_lo..=limit {
        let Some(&hash) = height_to_hash.get(&ht) else {
            break;
        };
        if body.is_rejected(&hash) {
            break;
        }
        flags.push(claim_ready(hub, body, ht, &hash));
    }
    tip_hole_from_claim_ready(&flags)
}

/// Build a status snapshot for the ~5s operator tick.
#[cfg(test)]
pub(crate) fn work_chain_progress(
    hub: &ChainHub,
    height_to_hash: &HashMap<u32, BlockHash>,
    body: &mut BodyPresence,
    horizon: u32,
    max_ready_height: u32,
) -> WorkChainProgress {
    work_chain_progress_at(
        hub,
        hub.tip_height(),
        height_to_hash,
        body,
        horizon,
        max_ready_height,
    )
}

pub(crate) fn work_chain_progress_at(
    hub: &ChainHub,
    tip: Option<u32>,
    height_to_hash: &HashMap<u32, BlockHash>,
    body: &mut BodyPresence,
    horizon: u32,
    max_ready_height: u32,
) -> WorkChainProgress {
    let tip_n = tip.unwrap_or(0);
    let tip_hole = tip_fetch_hole_at(hub, tip, height_to_hash, body);
    WorkChainProgress {
        tip: tip_n,
        ready_hwm: tip_n.max(max_ready_height),
        headers: tip_n.max(horizon),
        tip_hole,
    }
}

/// Numbers the 5s tick reads. No directory listing and no height-index lock.
pub(crate) struct ProgressPublished {
    pub tip: Option<u32>,
    pub txs: u64,
    pub sh_runs: usize,
    pub owned: rbitcoin_query::ProcessOwnedSizes,
}

pub(crate) fn read_progress_published(query: &rbitcoin_query::Query) -> ProgressPublished {
    ProgressPublished {
        tip: query.published_tip().map(|p| p.height),
        txs: query.tx_body_count(),
        sh_runs: query.published_scripthash_run_count(),
        owned: query.published_owned_sizes(),
    }
}

/// Confirmed tip as a percent of network/header horizon (0–100).
/// Denominator is max(our known headers, best peer-advertised tip).
pub(crate) fn ibd_pct(tip: u32, horizon: u32) -> u32 {
    let denom = horizon.max(tip).max(1);
    ((u64::from(tip) * 100) / u64::from(denom)) as u32
}

/// Half-life for the tip-rate EWMA used by ETA (seconds).
///
/// ~90s: recent IBD pace dominates (not the last hour of early empty blocks).
/// A single 5s spike only moves the estimate a few percent; a sustained regime
/// change is mostly reflected within ~5–6 minutes.
const ETA_RATE_HALF_LIFE_SECS: f64 = 90.0;

/// Need at least this much wall time of samples before publishing an ETA.
const ETA_MIN_ELAPSED_SECS: f64 = 20.0;

/// Present-biased smoothed tip rate for IBD ETA logs.
///
/// Samples are pushed on the centralized ~5s status tick. ETA uses an EWMA of
/// inter-sample rates (half-life [`ETA_RATE_HALF_LIFE_SECS`]): recent pace
/// dominates, but a single 5s spike only moves the estimate a few percent.
pub(crate) struct TipRateTracker {
    /// EWMA of tip advance rate (blocks/s).
    rate_ema: Option<f64>,
    /// Previous sample used to form the next instantaneous interval rate.
    last: Option<(Instant, u32)>,
    /// First sample time (gate ETA until we have a few ticks of history).
    first_at: Option<Instant>,
}

impl TipRateTracker {
    pub(crate) fn new() -> Self {
        Self {
            rate_ema: None,
            last: None,
            first_at: None,
        }
    }

    /// Record a sample (call once per centralized status tick).
    pub(crate) fn push(&mut self, now: Instant, tip: u32) {
        if self.first_at.is_none() {
            self.first_at = Some(now);
        }
        if let Some((t0, tip0)) = self.last {
            let dt = now.duration_since(t0).as_secs_f64();
            // Ignore zero/negative clock skew; tiny dt would explode inst rate.
            if dt >= 0.5 {
                let inst = tip.saturating_sub(tip0) as f64 / dt;
                let inst = if inst.is_finite() && inst >= 0.0 {
                    inst
                } else {
                    0.0
                };
                self.rate_ema = Some(match self.rate_ema {
                    None => inst,
                    Some(prev) => {
                        let alpha =
                            1.0 - (-std::f64::consts::LN_2 * dt / ETA_RATE_HALF_LIFE_SECS).exp();
                        let alpha = alpha.clamp(0.02, 0.5);
                        alpha * inst + (1.0 - alpha) * prev
                    }
                });
            }
        }
        self.last = Some((now, tip));
    }

    /// Smoothed tip rate (blocks/s) for ETA: present-biased EWMA.
    pub(crate) fn eta_rate(&self, now: Instant) -> Option<f64> {
        let first = self.first_at?;
        let elapsed = now.duration_since(first).as_secs_f64();
        if elapsed < ETA_MIN_ELAPSED_SECS {
            return None;
        }
        let ema = self.rate_ema.filter(|r| r.is_finite() && *r > 1e-6)?;
        Some(ema)
    }

    /// Human ETA from tip→horizon using smoothed present-biased tip rate.
    pub(crate) fn eta_string(&self, now: Instant, tip: u32, horizon: u32) -> String {
        let remain = horizon.saturating_sub(tip);
        if remain == 0 {
            return "done".into();
        }
        let Some(rate) = self.eta_rate(now) else {
            return "eta=?".into();
        };
        let secs = (remain as f64 / rate).ceil() as u64;
        format!("eta={}", format_duration_short(secs))
    }
}

/// Compact duration for logs: `45s`, `12m`, `3.2h`, `2d5h`.
pub(crate) fn format_duration_short(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3600 {
        return format!("{}m", (secs + 30) / 60);
    }
    if secs < 86400 {
        let h = secs as f64 / 3600.0;
        if h < 10.0 {
            return format!("{h:.1}h");
        }
        return format!("{}h", (secs + 1800) / 3600);
    }
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    if h == 0 {
        format!("{d}d")
    } else {
        format!("{d}d{h}h")
    }
}

/// Format a per-second rate for progress logs (`0`, `2.4`, `31`).
pub(crate) fn format_rate(rate: f64) -> String {
    if !rate.is_finite() || rate < 0.0 {
        return "0".into();
    }
    if rate < 10.0 {
        format!("{rate:.1}")
    } else {
        format!("{rate:.0}")
    }
}

/// One live peer's block-getdata watch. The status thread sends shutdown
/// on this copy. It does not take the coordination lock.
#[derive(Clone)]
pub(crate) struct WireWatch {
    pub cmd_tx: tokio::sync::mpsc::UnboundedSender<super::peer_io::PeerCmd>,
    pub abort: tokio::task::AbortHandle,
    pub in_flight: usize,
    pub getdata_wire: Arc<super::peer_io::GetdataWire>,
    pub rate: super::rate::PeerRate,
    pub alive: bool,
    pub addr: std::net::SocketAddr,
    pub bytes_rx: u64,
}

/// Numbers the status thread prints. The coordination loop publishes a new
/// copy when it runs. A stuck loop leaves this snapshot in place.
#[derive(Clone, Default)]
pub(crate) struct StatusSnap {
    pub peers: usize,
    pub horizon: u32,
    pub hole: usize,
    pub header_origin: bool,
    pub header_height: u32,
    pub header_stored: u32,
    pub header_queue: usize,
    pub header_work_ok: bool,
    pub header_walk: bool,
    pub header_quiet: bool,
    pub wires: Vec<WireWatch>,
}

/// Confirm-tip clock for the status thread. Wire stalls disconnect only
/// after this tip has stopped advancing.
pub(crate) struct StatusClock {
    tip: u32,
    moved: Instant,
    last_sample_tip: u32,
    last_sample_at: Instant,
    saw_quiet: bool,
    rate: TipRateTracker,
}

impl StatusClock {
    pub(crate) fn new(tip: u32, now: Instant) -> Self {
        let mut rate = TipRateTracker::new();
        rate.push(now, tip);
        Self {
            tip,
            moved: now,
            last_sample_tip: tip,
            last_sample_at: now,
            saw_quiet: false,
            rate,
        }
    }

    /// True when `tip` has not changed for longer than `stall`.
    pub(crate) fn tip_frozen(&mut self, tip: u32, now: Instant, stall: Duration) -> bool {
        if tip != self.tip {
            self.tip = tip;
            self.moved = now;
        }
        now.saturating_duration_since(self.moved) > stall
    }
}

fn wire_is_stalled(wire: &WireWatch, now_ms: u64, stall_ms: u64) -> bool {
    if !wire.alive || wire.in_flight == 0 {
        return false;
    }
    if wire.getdata_wire.unwritten() >= wire.in_flight {
        return false;
    }
    let mut rate = wire.rate;
    let written = wire.getdata_wire.written_ms();
    if written > rate.work_started_ms {
        rate.work_started_ms = written;
    }
    rate.stalled(now_ms, stall_ms, true)
}

/// Shutdown peers whose block getdata is already on the wire, but only when
/// the confirmed tip itself has stopped. A stuck coordination loop does not
/// look like a peer stall while confirm is still advancing.
pub(crate) fn drop_stalled_wires(
    wires: &[WireWatch],
    now_ms: u64,
    stall: Duration,
    tip_frozen: bool,
) -> Vec<std::net::SocketAddr> {
    if !tip_frozen {
        return Vec::new();
    }
    let stall_ms = stall.max(Duration::from_secs(30)).as_millis() as u64;
    let mut kicked = Vec::new();
    for wire in wires {
        if !wire_is_stalled(wire, now_ms, stall_ms) {
            continue;
        }
        rbitcoin_log::warn!(
            "ibd: peer {} stalled (no block progress for {stall:?}, {} in-flight, stream={}) — disconnect",
            wire.addr,
            wire.in_flight,
            wire.bytes_rx
        );
        let _ = wire.cmd_tx.send(super::peer_io::PeerCmd::Shutdown);
        wire.abort.abort();
        kicked.push(wire.addr);
    }
    kicked
}

/// One progress line from published counters and the last coordination snapshot.
pub(crate) fn log_progress_tick(
    clock: &mut StatusClock,
    snap: &StatusSnap,
    published: &ProgressPublished,
    queues: (usize, usize, usize),
    bq_count: usize,
    bq_bytes: u64,
    now: Instant,
) {
    let tip = published.tip.unwrap_or(0);
    clock.rate.push(now, tip);
    let horizon = snap.horizon.max(tip);
    let pct = ibd_pct(tip, horizon);
    let window = now
        .saturating_duration_since(clock.last_sample_at)
        .as_secs_f64()
        .max(0.001);
    let tip_rate = tip.saturating_sub(clock.last_sample_tip) as f64 / window;
    clock.last_sample_tip = tip;
    clock.last_sample_at = now;
    let eta_rate = clock.rate.eta_rate(now);
    let eta = clock.rate.eta_string(now, tip, horizon);
    let conf_q = super::confirm::format_conf_q(
        queues.0,
        queues.1,
        queues.2,
        super::confirm::load_queue_cap(),
        super::confirm::script_queue_cap(),
        super::confirm::write_queue_cap(),
    );
    let line = format_progress_line(&ProgressLineInput {
        pct,
        tip,
        tip_rate,
        tip_hole: snap.hole,
        peers: snap.peers,
        conf_q,
        txs: published.txs,
        horizon,
        eta,
        bq_bytes,
        bq_count,
        bq_soft_stop: rbitcoin_query::soft_confirm_window_n(eta_rate),
    });
    rbitcoin_log::info_bold!("{line}");
    if !snap.header_origin || snap.header_quiet {
        if snap.header_quiet && !clock.saw_quiet {
            rbitcoin_log::info!(
                "ibd: headers done height={} horizon={horizon}",
                snap.header_height
            );
        }
        clock.saw_quiet = snap.header_quiet;
        return;
    }
    clock.saw_quiet = false;
    let pct_h = ibd_pct(snap.header_height, horizon.max(snap.header_height));
    let work = if snap.header_work_ok { "ok" } else { "below" };
    let phase = if snap.header_walk { "walk" } else { "write" };
    rbitcoin_log::info!(
        "ibd: headers height={} ({pct_h}%) horizon={horizon} stored={} queue={} work={work} phase={phase}",
        snap.header_height,
        snap.header_stored,
        snap.header_queue
    );
}

/// Snapshot plus stall kicks the coordination loop applies when it next runs.
pub(crate) struct StatusBoard {
    snap: std::sync::Mutex<StatusSnap>,
    kicks: std::sync::Mutex<Vec<std::net::SocketAddr>>,
    hole: std::sync::atomic::AtomicUsize,
}

impl StatusBoard {
    pub(crate) fn new() -> Self {
        Self {
            snap: std::sync::Mutex::new(StatusSnap::default()),
            kicks: std::sync::Mutex::new(Vec::new()),
            hole: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(crate) fn publish(&self, snap: StatusSnap) {
        *self.snap.lock().unwrap_or_else(|e| e.into_inner()) = snap;
    }

    pub(crate) fn set_hole(&self, hole: usize) {
        self.hole.store(hole, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn take_kicks(&self) -> Vec<std::net::SocketAddr> {
        std::mem::take(&mut *self.kicks.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn note_kicks(&self, addrs: Vec<std::net::SocketAddr>) {
        if addrs.is_empty() {
            return;
        }
        self.kicks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(addrs);
    }
}

/// Drop remembered kicks whose address is no longer in the snapshot.
///
/// A kick stays remembered while that peer is still published, so the
/// status thread does not shut the same socket down every 5 seconds.
/// Once the coordination loop has dropped the peer, a later redial of
/// the same address can stall and be disconnected again.
fn forget_departed_addrs(
    dropped: &mut std::collections::HashSet<std::net::SocketAddr>,
    wires: &[WireWatch],
) {
    let live: std::collections::HashSet<_> = wires.iter().map(|w| w.addr).collect();
    dropped.retain(|addr| live.contains(addr));
}

pub(crate) fn spawn_status_thread(
    board: Arc<StatusBoard>,
    query: Arc<rbitcoin_query::Query>,
    queues: Arc<super::confirm::ConfirmQueueDepths>,
    stall: Duration,
    start_tip: u32,
    stop: std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("ibd-status".into())
        .spawn(move || {
            let mut clock = StatusClock::new(start_tip, Instant::now());
            let mut dropped = std::collections::HashSet::new();
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                stop.recv_timeout(Duration::from_secs(5))
            {
                let mut snap = board.snap.lock().unwrap_or_else(|e| e.into_inner()).clone();
                snap.hole = board.hole.load(std::sync::atomic::Ordering::Relaxed);
                let published = read_progress_published(&query);
                let (_budget, bq_bytes, bq_count) = query.block_queue_stats();
                let now = Instant::now();
                log_progress_tick(
                    &mut clock,
                    &snap,
                    &published,
                    queues.snap(),
                    bq_count,
                    bq_bytes,
                    now,
                );
                let tip = published.tip.unwrap_or(0);
                let frozen = clock.tip_frozen(tip, now, stall);
                forget_departed_addrs(&mut dropped, &snap.wires);
                snap.wires.retain(|w| !dropped.contains(&w.addr));
                let kicked =
                    drop_stalled_wires(&snap.wires, super::peer_io::ibd_mono_ms(), stall, frozen);
                dropped.extend(kicked.iter().copied());
                board.note_kicks(kicked);
            }
        })
        .expect("ibd-status thread")
}

/// Pure tip-hole count over tip+1.. using claim-ready flags.
///
/// `claim_ready[i]` corresponds to height path_lo+i. Production
/// [`tip_fetch_hole`] walks heights and builds the same sequential hole count.
pub(crate) fn tip_hole_from_claim_ready(claim_ready_flags: &[bool]) -> usize {
    let mut tip_hole = 0usize;
    for &ready in claim_ready_flags {
        if ready {
            break;
        }
        tip_hole += 1;
    }
    tip_hole
}

#[cfg(test)]
mod tests {
    use super::{
        format_duration_short, format_progress_line, format_rate, ibd_pct,
        tip_hole_from_claim_ready, ProgressLineInput, TipRateTracker,
    };
    use rbitcoin_query::{
        soft_assign_restricted, soft_confirm_window_n, soft_densify_band_hi, BQ_SOFT_FREE_BYTES,
    };
    use std::time::{Duration, Instant};

    /// Shipped progress line: tip + in-RAM bq soft depth; forbid retired tokens.
    #[test]
    fn format_progress_line_schema12_tokens() {
        // tip_rate 12.5 → format_rate rounds to "12" (≥10).
        let line = format_progress_line(&ProgressLineInput {
            pct: 42,
            tip: 100_000,
            tip_rate: 12.5,
            tip_hole: 3,
            peers: 8,
            conf_q: "ready=1 scriptq=1/2 writeq=0/2".into(),
            txs: 50_000_000,
            horizon: 900_000,
            eta: "eta=18h".into(),
            bq_bytes: 256 * 1024 * 1024,
            bq_count: 17,
            bq_soft_stop: 180,
        });
        assert_eq!(
            line,
            "ibd: progress 42% tip=100000 (12/s) hole=3 peers=8 ready=1 scriptq=1/2 writeq=0/2 txs=50000000 horizon=900000 eta=18h bq soft=17/180 RAM=256MiB"
        );
        // Current schema tokens present.
        assert!(line.contains(" hole="), "{line}");
        assert!(line.contains(" soft="), "{line}");
        assert!(line.contains(" RAM="), "{line}");
        assert!(
            !line.contains(" bq n="),
            "count lives in soft= only: {line}"
        );
        assert!(!line.contains(" disk="), "queue is RAM not disk: {line}");
        assert!(
            !line.contains("pending_ram="),
            "no RAM overflow meter: {line}"
        );
        assert!(line.contains("ready="), "{line}");
        assert!(line.contains("scriptq"), "{line}");
        // Retired dual-track progress tokens forbidden.
        assert!(
            !line.contains("arch_hwm"),
            "must not report retired arch_hwm: {line}"
        );
        assert!(
            !line.contains("lead="),
            "must not report retired lead=: {line}"
        );
        assert!(
            !line.contains("arch="),
            "must not report retired arch= rate token: {line}"
        );
        assert_eq!(
            line.matches("/s)").count(),
            1,
            "only tip rate on progress line: {line}"
        );
        let slow = format_progress_line(&ProgressLineInput {
            pct: 1,
            tip: 10,
            tip_rate: 2.4,
            tip_hole: 0,
            peers: 1,
            conf_q: "ready=0 scriptq<0/2 writeq<0/2".into(),
            txs: 1,
            horizon: 1000,
            eta: "eta=?".into(),
            bq_bytes: 0,
            bq_count: 0,
            bq_soft_stop: 256,
        });
        assert!(slow.contains("tip=10 (2.4/s)"), "{slow}");
        assert!(slow.contains("bq soft=0/256 RAM=0MiB"), "{slow}");
        assert!(
            !slow.contains("arch_hwm") && !slow.contains("lead="),
            "{slow}"
        );
    }

    #[test]
    fn soft_confirm_window_and_band_at_rate() {
        assert_eq!(soft_confirm_window_n(Some(2.0)), 120, "2 blk/s × 60s");
        assert_eq!(soft_confirm_window_n(None), 0, "cold rate");
        assert_eq!(soft_confirm_window_n(Some(5.0)), 300, "5 blk/s × 60s");
        assert_eq!(soft_confirm_window_n(Some(10.0)), 600);

        let free = BQ_SOFT_FREE_BYTES;
        let over = free + 1;
        assert_eq!(
            soft_densify_band_hi(10, 5000, free, Some(5.0), u64::MAX, None),
            5000
        );
        assert_eq!(
            soft_densify_band_hi(10, 5000, over, Some(5.0), u64::MAX, None),
            309
        );
        assert_eq!(
            soft_densify_band_hi(10, 5000, over, None, u64::MAX, None),
            10
        );
        assert!(!soft_assign_restricted(free));
        assert!(soft_assign_restricted(over));
    }

    #[allow(clippy::cognitive_complexity)] // one fixture, many progress arms
    #[test]
    fn pct_tip_hole_and_format_surface() {
        assert_eq!(ibd_pct(0, 100), 0);
        assert_eq!(ibd_pct(50, 100), 50);
        assert_eq!(ibd_pct(100, 100), 100);
        assert_eq!(ibd_pct(200, 100), 100);
        assert_eq!(ibd_pct(0, 0), 0);
        assert_eq!(ibd_pct(5, 0), 100);

        // claim_ready flags from tip+1: false,false,true → hole=2
        assert_eq!(tip_hole_from_claim_ready(&[]), 0);
        assert_eq!(tip_hole_from_claim_ready(&[true, false, false]), 0);
        assert_eq!(tip_hole_from_claim_ready(&[false, false, true, false]), 2);
        assert_eq!(tip_hole_from_claim_ready(&[false, false, false]), 3);

        assert_eq!(format_duration_short(45), "45s");
        assert_eq!(format_duration_short(59), "59s");
        assert_eq!(format_duration_short(60), "1m");
        assert_eq!(format_duration_short(120), "2m");
        assert_eq!(format_duration_short(3_600), "1.0h");
        assert_eq!(format_duration_short(9 * 3600), "9.0h");
        assert_eq!(format_duration_short(36_000), "10h");
        assert_eq!(format_duration_short(86_400), "1d");
        assert_eq!(format_duration_short(2 * 86_400), "2d");
        assert_eq!(format_duration_short(90_000), "1d1h");
        assert_eq!(format_rate(0.0), "0.0");
        assert_eq!(format_rate(2.4), "2.4");
        assert_eq!(format_rate(9.9), "9.9");
        assert_eq!(format_rate(10.0), "10");
        assert_eq!(format_rate(31.2), "31");
        assert_eq!(format_rate(f64::NAN), "0");
        assert_eq!(format_rate(f64::INFINITY), "0");
        assert_eq!(format_rate(-1.0), "0");

        let mut done = TipRateTracker::new();
        let t0 = Instant::now();
        done.push(t0, 100);
        assert_eq!(
            done.eta_string(t0 + Duration::from_secs(30), 100, 100),
            "done"
        );
    }

    #[test]
    fn work_chain_progress_fetch_hole_requires_body_queue() {
        use super::super::body::BodyPresence;
        use super::{claim_ready, tip_fetch_hole, work_chain_progress};
        use bitcoin::hashes::Hash;
        use bitcoin::BlockHash;

        use std::collections::HashMap;

        let (dir, hub) = crate::chain::tiny_regtest_hub_labeled("wcp");
        hub.ensure_genesis().unwrap();

        // tip=0 (genesis) → path_lo=1. Heights 1..3 missing from BQ.
        let mut h2h = HashMap::new();
        let mut body = BodyPresence::new();
        let h1 = BlockHash::from_byte_array([1u8; 32]);
        let h2 = BlockHash::from_byte_array([2u8; 32]);
        let h3 = BlockHash::from_byte_array([3u8; 32]);
        h2h.insert(1, h1);
        h2h.insert(2, h2);
        h2h.insert(3, h3);
        body.mark_missing(h1);
        body.mark_missing(h2);
        // Zombie pending without BQ must **not** look claim-ready (mainnet stall).
        body.mark_pending(h3);
        assert!(
            !claim_ready(&hub, &mut body, 3, &h3),
            "pending alone without body queue is not claim-ready"
        );

        let p = work_chain_progress(&hub, &h2h, &mut body, 50, 10);
        assert_eq!(p.tip, 0);
        assert_eq!(
            p.tip_hole, 3,
            "zombie pending at h=3 still counts as fetch hole"
        );

        // Zombie pending at tip+1 → still hole (must re-getdata).
        body.mark_pending(h1);
        let p2 = work_chain_progress(&hub, &h2h, &mut body, 50, 10);
        assert_eq!(p2.tip_hole, 3);

        // Rejected tip+1 stops the hole walk.
        body.mark_rejected(h1);
        assert!(!claim_ready(&hub, &mut body, 1, &h1));
        let p3 = work_chain_progress(&hub, &h2h, &mut body, 50, 10);
        assert_eq!(p3.tip_hole, 0, "rejected tip+1 is not a download hole");

        // tip_fetch_hole with empty height map (no headers past tip).
        let hole0 = tip_fetch_hole(&hub, &HashMap::new(), &mut body);
        assert_eq!(hole0, 0);

        // Gap in height map stops walk (missing header on path).
        let mut sparse = HashMap::new();
        sparse.insert(1, h1);
        // no height 2 → break after counting hole for h1 if not ready
        let mut body_gap = BodyPresence::new();
        body_gap.mark_missing(h1);
        let hole_gap = tip_fetch_hole(&hub, &sparse, &mut body_gap);
        assert_eq!(hole_gap, 1);

        // BQ claim-ready at tip+1 stops hole after reset body.
        let mut body2 = BodyPresence::new();
        body2.mark_missing(h1);
        hub.query
            .block_queue_enqueue(1, h1.to_byte_array(), 1, b"x")
            .unwrap();
        assert!(claim_ready(&hub, &mut body2, 1, &h1));
        let hole_ready = tip_fetch_hole(&hub, &h2h, &mut body2);
        assert_eq!(hole_ready, 0, "BQ-ready tip+1 → hole=0");

        // Later gap at height 5 is not operator hole= (prefix is in hand).
        let h5 = BlockHash::from_byte_array([5u8; 32]);
        h2h.insert(5, h5);
        body2.mark_missing(h5);
        let hole_later = tip_fetch_hole(&hub, &h2h, &mut body2);
        assert_eq!(hole_later, 0, "only +5 missing after ready tip+1 → hole=0");

        // Confirmed tip block is claim-ready (hub.has_block).
        let ghash = hub.tip_hash().expect("genesis tip");
        assert!(claim_ready(&hub, &mut body2, 0, &ghash));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claim_ready_taken_prefix_is_not_a_fetch_hole() {
        use super::super::body::BodyPresence;
        use super::{claim_ready, tip_fetch_hole};
        use bitcoin::hashes::Hash;
        use bitcoin::BlockHash;

        use std::collections::HashMap;

        let (dir, hub) = crate::chain::tiny_regtest_hub_labeled("taken-hole");
        hub.ensure_genesis().unwrap();

        let mut h2h = HashMap::new();
        let mut body = BodyPresence::new();
        for i in 1u8..=4 {
            let h = BlockHash::from_byte_array([i; 32]);
            h2h.insert(i as u32, h);
            body.mark_missing(h);
        }
        hub.query.set_lookup_taken_hi(Some(2));
        assert!(claim_ready(&hub, &mut body, 1, &h2h[&1]));
        assert!(claim_ready(&hub, &mut body, 2, &h2h[&2]));
        assert!(!claim_ready(&hub, &mut body, 3, &h2h[&3]));
        assert_eq!(tip_fetch_hole(&hub, &h2h, &mut body), 0);

        hub.query.set_lookup_taken_hi(None);
        assert!(!claim_ready(&hub, &mut body, 1, &h2h[&1]));
        assert_eq!(tip_fetch_hole(&hub, &h2h, &mut body), 4);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// EWMA tip-rate: warmup gate, steady ETA.
    #[test]
    fn tip_rate_tracker_eta_surface() {
        let t0 = Instant::now();

        let mut cold = TipRateTracker::new();
        cold.push(t0, 0);
        cold.push(t0 + Duration::from_secs(5), 50);
        assert!(cold.eta_rate(t0 + Duration::from_secs(5)).is_none());
        assert_eq!(
            cold.eta_string(t0 + Duration::from_secs(5), 50, 1_000_000),
            "eta=?"
        );

        let mut steady = TipRateTracker::new();
        for i in 0u32..=60 {
            steady.push(t0 + Duration::from_secs(u64::from(i) * 5), 100 + i * 5);
        }
        let now = t0 + Duration::from_secs(300);
        let rate = steady.eta_rate(now).expect("warmed");
        assert!((rate - 1.0).abs() < 0.15, "rate={rate}");
        // ~7200 remain at ~1 blk/s → ~2h
        let eta = steady.eta_string(now, 400, 7_600);
        assert!(eta.starts_with("eta="), "{eta}");
        assert!(eta.contains('h') || eta.contains('m'), "{eta}");

        // Sub-0.5s dt does not update EMA; cold tracker still eta=?
        let mut tiny = TipRateTracker::new();
        tiny.push(t0, 10);
        tiny.push(t0 + Duration::from_millis(100), 11);
        assert!(tiny.eta_rate(t0 + Duration::from_millis(100)).is_none());
        // tip >= horizon → "done"
        let mut warm = TipRateTracker::new();
        for i in 0u32..=40 {
            warm.push(t0 + Duration::from_secs(u64::from(i) * 2), i * 2);
        }
        assert_eq!(
            warm.eta_string(t0 + Duration::from_secs(100), 10_000, 100),
            "done"
        );
        assert_eq!(
            warm.eta_string(t0 + Duration::from_secs(100), 100, 100),
            "done"
        );
    }

    #[test]
    fn progress_tick_reads_published_counters() {
        use bitcoin::hashes::Hash;

        let (dir, hub) = crate::chain::tiny_regtest_hub_labeled("prog-pub");
        hub.ensure_genesis().unwrap();
        let gen = hub.tip_hash().unwrap();
        hub.query.publish_confirmed_tip(0, gen.to_byte_array());
        rbitcoin_query::reset_progress_snapshot_counts();
        hub.query.publish_progress_snapshots();
        let (sh_calls, owned_calls) = rbitcoin_query::progress_snapshot_counts();
        assert!(sh_calls >= 1 && owned_calls >= 1);
        let sh = hub.query.published_scripthash_run_count();
        let h2h = hub.query.published_owned_sizes().h2h_keys;
        rbitcoin_query::reset_progress_snapshot_counts();
        let view = super::read_progress_published(&hub.query);
        assert_eq!(view.tip, Some(0));
        assert_eq!(view.txs, hub.query.tx_body_count());
        assert_eq!(view.sh_runs, sh);
        assert_eq!(view.owned.h2h_keys, h2h);
        assert_eq!(rbitcoin_query::progress_snapshot_counts(), (0, 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn progress_tick_logs_a_walk_and_a_quiet_header_line() {
        let t0 = Instant::now();
        let mut clock = super::StatusClock::new(0, t0);
        let published = super::ProgressPublished {
            tip: Some(10),
            txs: 4,
            sh_runs: 0,
            owned: rbitcoin_query::ProcessOwnedSizes::default(),
        };
        let mut snap = super::StatusSnap {
            peers: 1,
            horizon: 100,
            hole: 2,
            header_origin: true,
            header_height: 40,
            header_stored: 30,
            header_queue: 3,
            header_work_ok: false,
            header_walk: true,
            header_quiet: false,
            ..super::StatusSnap::default()
        };
        super::log_progress_tick(
            &mut clock,
            &snap,
            &published,
            (0, 0, 0),
            1,
            2,
            t0 + Duration::from_secs(5),
        );
        assert!(!clock.saw_quiet, "a walking header line is not caught up");
        snap.header_work_ok = true;
        snap.header_walk = false;
        snap.header_quiet = true;
        super::log_progress_tick(
            &mut clock,
            &snap,
            &published,
            (0, 0, 0),
            1,
            2,
            t0 + Duration::from_secs(10),
        );
        assert!(clock.saw_quiet, "the caught-up line fires once");
        super::log_progress_tick(
            &mut clock,
            &snap,
            &published,
            (0, 0, 0),
            1,
            2,
            t0 + Duration::from_secs(15),
        );
        assert!(
            clock.saw_quiet,
            "a second quiet tick does not clear the latch"
        );
        snap.header_quiet = false;
        snap.header_origin = false;
        let unpublished = super::ProgressPublished {
            tip: None,
            txs: 0,
            sh_runs: 0,
            owned: rbitcoin_query::ProcessOwnedSizes::default(),
        };
        super::log_progress_tick(
            &mut clock,
            &snap,
            &unpublished,
            (0, 0, 0),
            0,
            0,
            t0 + Duration::from_secs(20),
        );
        assert!(
            !clock.saw_quiet,
            "leaving the walk clears the caught-up latch"
        );
    }

    #[tokio::test]
    async fn a_redial_of_a_kicked_addr_can_stall_again() {
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let other: std::net::SocketAddr = "127.0.0.1:2".parse().unwrap();
        let mut dropped = std::collections::HashSet::new();
        dropped.insert(addr);
        let still = [watch_addr(addr)];
        super::forget_departed_addrs(&mut dropped, &still);
        assert!(
            dropped.contains(&addr),
            "a kicked peer still in the snapshot is not shut down again"
        );
        let gone = [watch_addr(other)];
        super::forget_departed_addrs(&mut dropped, &gone);
        assert!(
            !dropped.contains(&addr),
            "once the peer leaves the snapshot, a redial of that address can stall again"
        );
        still[0].abort.abort();
        gone[0].abort.abort();
    }

    fn watch_addr(addr: std::net::SocketAddr) -> super::WireWatch {
        let (cmd_tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let task: tokio::task::JoinHandle<()> = tokio::spawn(std::future::pending());
        let abort = task.abort_handle();
        super::WireWatch {
            cmd_tx,
            abort,
            in_flight: 1,
            getdata_wire: super::super::peer_io::GetdataWire::new(),
            rate: super::super::rate::PeerRate::default(),
            alive: true,
            addr,
            bytes_rx: 0,
        }
    }

    fn stalled_wire() -> (
        super::WireWatch,
        tokio::sync::mpsc::UnboundedReceiver<super::super::peer_io::PeerCmd>,
        tokio::task::JoinHandle<()>,
    ) {
        let (cmd_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(std::future::pending());
        let abort = task.abort_handle();
        let wire = super::WireWatch {
            cmd_tx,
            abort,
            in_flight: 1,
            getdata_wire: super::super::peer_io::GetdataWire::new(),
            rate: super::super::rate::PeerRate::default(),
            alive: true,
            addr: "127.0.0.1:1".parse().unwrap(),
            bytes_rx: 0,
        };
        (wire, rx, task)
    }

    #[tokio::test]
    async fn moving_confirm_tip_does_not_drop_a_stalled_wire() {
        let (wire, mut rx, task) = stalled_wire();
        let kicked = super::drop_stalled_wires(&[wire], 60_000, Duration::from_secs(30), false);
        assert!(kicked.is_empty());
        assert!(rx.try_recv().is_err());
        task.abort();
    }

    #[tokio::test]
    async fn frozen_confirm_tip_drops_a_stalled_wire() {
        let (wire, mut rx, _task) = stalled_wire();
        let mut clock = super::StatusClock::new(10, Instant::now() - Duration::from_secs(60));
        assert!(clock.tip_frozen(10, Instant::now(), Duration::from_secs(30)));
        let kicked = super::drop_stalled_wires(&[wire], 60_000, Duration::from_secs(30), true);
        assert_eq!(kicked.len(), 1);
        assert!(matches!(
            rx.try_recv(),
            Ok(super::super::peer_io::PeerCmd::Shutdown)
        ));
    }
}
