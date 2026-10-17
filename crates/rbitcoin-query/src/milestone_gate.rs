//! Confirm-path milestone gate. The predicate is [`rbitcoin_consensus::Milestone::skips_scripts`].

use rbitcoin_consensus::Milestone;

use crate::Query;

/// Notes `milestone_gate_ns` (`ibd: perf_dbg milestone_us`).
pub fn skips_on_query(milestone: Milestone, query: &Query, height: u32, block: &[u8; 32]) -> bool {
    let t = std::time::Instant::now();
    let skip = milestone.skips_scripts(height, block, |h| query.milestone_header_at(h));
    crate::note_confirm(
        &query.confirm_stats().milestone_gate_ns,
        t.elapsed().as_nanos() as u64,
    );
    skip
}

pub fn check_scripts(milestone: Milestone, query: &Query, height: u32, block: &[u8; 32]) -> bool {
    !skips_on_query(milestone, query, height, block)
}
