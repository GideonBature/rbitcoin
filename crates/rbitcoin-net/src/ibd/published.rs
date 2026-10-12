//! Header rows the coordination loop copies. A header thread does the store IO.

#[cfg(test)]
use super::dial::disconnect_stalled_block_peers_at;
use super::dial::set_warm_locator;
use super::events::{
    apply_headers_event, clear_prepared_batch, prepare_header_store, set_apply_inline,
    set_header_store_prepared, set_prepared_batch,
};
use super::header_walk::{install_origin, load_origin, origin_input, send_getheaders};
use super::path::{install_resumed_path, load_resume_entries, push_reseed, reseed_candidates};
use super::state::{set_defer_header_store, HeaderCmd, IbdWorkState};
use crate::chain::ChainHub;
use bitcoin::BlockHash;
use rbitcoin_primitives::Fk;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
#[cfg(test)]
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderPathRow {
    pub hash: BlockHash,
    pub height: u32,
    pub prev: BlockHash,
    pub fk: Fk,
}

#[cfg(test)]
pub(crate) fn apply_published_header_rows(st: &mut IbdWorkState, rows: &[HeaderPathRow]) {
    for row in rows {
        // Already copied, then removed from the queue (confirm, reject, rewind).
        // Replaying the snapshot would put the hash back and clobber the height.
        if st.known_headers.contains(&row.hash) && !st.ordered_set.contains(&row.hash) {
            continue;
        }
        if st
            .height_to_hash
            .get(&row.height)
            .is_some_and(|occupant| *occupant != row.hash)
        {
            continue;
        }
        st.header_fks.insert(row.hash, row.fk);
        st.note_path_prev(row.hash, row.prev);
        st.record_height(row.hash, row.height);
        st.known_headers.insert(row.hash);
        st.max_ordered_height = st.max_ordered_height.max(row.height);
        st.max_peer_height = st.max_peer_height.max(row.height);
        if st.ordered_set.insert(row.hash) {
            st.ordered.push_back(row.hash);
        }
    }
}

/// Copy published rows, then the stall check. No header-store reads.
#[cfg(test)]
pub(crate) fn apply_published_and_stall(
    st: &mut IbdWorkState,
    rows: &[HeaderPathRow],
    now: Instant,
    stall: Duration,
    now_ms: u64,
) {
    apply_published_header_rows(st, rows);
    let freed = disconnect_stalled_block_peers_at(
        &mut st.slots,
        &mut st.inflight,
        &mut st.body,
        &mut st.addr_cooldown,
        &mut st.addr_strikes,
        now,
        stall,
        now_ms,
    );
    st.reopen_for_densify(&freed);
}

/// Fks for hashes that joined `ordered` during this apply. The caller publishes
/// them after it drops the coordination lock.
fn new_header_fks(st: &IbdWorkState, before: &HashSet<BlockHash>) -> Vec<(BlockHash, Fk)> {
    let mut rows = Vec::new();
    for hash in &st.ordered {
        if before.contains(hash) {
            continue;
        }
        let Some(fk) = st.header_fks.get(hash).copied() else {
            continue;
        };
        rows.push((*hash, fk));
    }
    rows
}

fn note_header_rows(query: &rbitcoin_query::Query, st: &IbdWorkState) {
    query.publish_header_rows(st.ordered.len() as u64);
}

pub(crate) fn spawn_header_thread(
    shared: Arc<Mutex<IbdWorkState>>,
    hub: Arc<ChainHub>,
    mut rx: Receiver<HeaderCmd>,
    header_fks: super::peer_io::HeaderFkBus,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                HeaderCmd::Batch { peer, headers } => {
                    let (fks, work) = prepare_header_store(hub.as_ref(), &headers);
                    set_prepared_batch(fks, work);
                    set_header_store_prepared(true);
                    set_apply_inline(true);
                    set_defer_header_store(true);
                    let pairs = {
                        let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        let before: HashSet<BlockHash> = st.ordered.iter().copied().collect();
                        apply_headers_event(&mut st, hub.as_ref(), peer, headers);
                        note_header_rows(&hub.query, &st);
                        new_header_fks(&st, &before)
                    };
                    set_defer_header_store(false);
                    set_apply_inline(false);
                    set_header_store_prepared(false);
                    clear_prepared_batch();
                    super::peer_io::publish_header_fks(&header_fks, &pairs);
                    finish_deferred(&shared, &hub);
                }
                HeaderCmd::Ask => {
                    let input = {
                        let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        origin_input(&st, hub.as_ref())
                    };
                    let loaded = if input.already {
                        None
                    } else {
                        Some(load_origin(hub.as_ref(), &input))
                    };
                    set_warm_locator(hub.query.locator_hashes().ok());
                    {
                        let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        if let Some(loaded) = loaded {
                            install_origin(&mut st, hub.as_ref(), loaded);
                        }
                        set_defer_header_store(true);
                        let _ = send_getheaders(&mut st, hub.as_ref());
                        set_defer_header_store(false);
                    }
                    set_warm_locator(None);
                    finish_deferred(&shared, &hub);
                }
                HeaderCmd::Reseed => {
                    let exclude: Vec<[u8; 32]> = {
                        let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        st.reorg.invalid.iter().collect()
                    };
                    let path = load_resume_entries(hub.as_ref(), &exclude);
                    let pairs = {
                        let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        let before: HashSet<BlockHash> = st.ordered.iter().copied().collect();
                        set_defer_header_store(true);
                        install_resumed_path(&mut st, hub.as_ref(), &path);
                        set_defer_header_store(false);
                        note_header_rows(&hub.query, &st);
                        new_header_fks(&st, &before)
                    };
                    super::peer_io::publish_header_fks(&header_fks, &pairs);
                    finish_deferred(&shared, &hub);
                }
            }
        }
    })
}

/// Store work the locked apply postponed: resume, origin, reseed filter, rewind.
fn finish_deferred(shared: &Arc<Mutex<IbdWorkState>>, hub: &Arc<ChainHub>) {
    for _ in 0..4 {
        let (origin, resume, rewind, reseed) = {
            let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
            let flags = (
                st.defer_origin,
                st.defer_resume,
                st.defer_rewind,
                st.defer_reseed,
            );
            st.defer_origin = false;
            st.defer_resume = false;
            st.defer_rewind = false;
            st.defer_reseed = false;
            flags
        };
        if !(origin || resume || rewind || reseed) {
            break;
        }
        if resume {
            let exclude: Vec<[u8; 32]> = {
                let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                st.reorg.invalid.iter().collect()
            };
            let path = load_resume_entries(hub.as_ref(), &exclude);
            let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
            set_defer_header_store(true);
            install_resumed_path(&mut st, hub.as_ref(), &path);
            set_defer_header_store(false);
        }
        if origin {
            let input = {
                let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                origin_input(&st, hub.as_ref())
            };
            if !input.already {
                let loaded = load_origin(hub.as_ref(), &input);
                let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                install_origin(&mut st, hub.as_ref(), loaded);
            }
        }
        if reseed {
            let (cands, rejected) = {
                let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                let tip_h = hub.tip_height().unwrap_or(0);
                let cands = reseed_candidates(&st, tip_h);
                let rejected: HashSet<BlockHash> = cands
                    .iter()
                    .copied()
                    .filter(|h| st.body.is_rejected(h))
                    .collect();
                (cands, rejected)
            };
            let keep: Vec<BlockHash> = cands
                .into_iter()
                .filter(|h| !rejected.contains(h) && !hub.has_block(h))
                .collect();
            let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
            push_reseed(&mut st, &keep);
        }
        if rewind {
            if let Err(e) = super::reorg::rewind_off_coord_lock(shared, hub.as_ref()) {
                rbitcoin_log::warn!("ibd: header-thread rewind failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::assign::tests::dummy_slot;
    use super::super::state::{IbdWorkState, InflightReq};
    use super::{apply_published_and_stall, apply_published_header_rows, HeaderPathRow};
    use crate::chain::{header_store_counts, reset_header_store_counts, tiny_regtest_hub_labeled};
    use bitcoin::hashes::Hash;
    use bitcoin::BlockHash;
    use rbitcoin_primitives::Fk;
    use std::time::{Duration, Instant};

    #[test]
    fn coord_loop_applies_published_header_rows() {
        let (dir, hub) = tiny_regtest_hub_labeled("coord-rows");
        let _ = hub.ensure_headers_batch(&[]);
        let _ = hub.header_of(&BlockHash::from_byte_array([0u8; 32]));
        let _ = hub.chain_work();
        let _ = hub
            .query
            .resume_work_path_after_tip_excluding([0u8; 32], 0, 1, &[]);
        reset_header_store_counts();
        rbitcoin_query::reset_resume_work_path_count();
        assert_eq!(header_store_counts(), (0, 0, 0));
        assert_eq!(rbitcoin_query::resume_work_path_count(), 0);

        let mut slot = dummy_slot(7);
        let owed = BlockHash::from_byte_array([0xee; 32]);
        slot.in_flight.insert(owed);
        slot.getdata_wire.wrote(1, 1_000);
        let mut st = IbdWorkState::new(vec![slot], None, Some(0));
        st.inflight.insert(owed, InflightReq::new(7));
        let row = HeaderPathRow {
            hash: BlockHash::from_byte_array([0x11; 32]),
            height: 1,
            prev: BlockHash::from_byte_array([0u8; 32]),
            fk: Fk(9),
        };
        apply_published_and_stall(
            &mut st,
            &[row],
            Instant::now(),
            Duration::from_secs(30),
            60_000,
        );
        assert_eq!(st.ordered.front().copied(), Some(row.hash));
        assert_eq!(st.height_to_hash.get(&1), Some(&row.hash));
        assert_eq!(st.path_prev.get(&row.hash), Some(&row.prev));
        assert!(!st.slots[0].alive, "stalled peer disconnects on this pass");
        assert_eq!(
            header_store_counts(),
            (0, 0, 0),
            "the pass does not read headers"
        );
        assert_eq!(rbitcoin_query::resume_work_path_count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn consumed_header_row_is_not_on_the_coord_pass() {
        let mut st = IbdWorkState::new(Vec::new(), None, Some(0));
        let row = HeaderPathRow {
            hash: BlockHash::from_byte_array([0x11; 32]),
            height: 1,
            prev: BlockHash::from_byte_array([0u8; 32]),
            fk: Fk(9),
        };
        apply_published_header_rows(&mut st, &[row]);
        st.ordered.pop_front();
        st.ordered_set.remove(&row.hash);
        let other = BlockHash::from_byte_array([0x22; 32]);
        st.record_height(other, 1);
        apply_published_header_rows(&mut st, &[row]);
        assert!(
            st.ordered.is_empty(),
            "replaying a consumed header must not put it back on the queue"
        );
        assert_eq!(
            st.height_to_hash.get(&1),
            Some(&other),
            "replaying a consumed header must not replace the height occupant"
        );
    }
}
