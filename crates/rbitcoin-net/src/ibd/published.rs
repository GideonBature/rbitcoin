//! Header rows the coordination loop copies. A header thread does the store IO.

#[cfg(test)]
use super::dial::disconnect_stalled_block_peers_at;
use super::dial::set_warm_locator;
use super::events::{
    apply_headers_event, clear_prepared_batch, set_apply_inline, set_prepared_batch,
};
use super::header_walk::{install_origin, load_origin, origin_input, send_getheaders};
use super::path::{install_resumed_path, load_resume_entries};
use super::state::{HeaderCmd, IbdWorkState};
use crate::chain::ChainHub;
use arc_swap::ArcSwap;
use bitcoin::BlockHash;
use rbitcoin_primitives::Fk;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
#[cfg(test)]
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderPathRow {
    pub hash: BlockHash,
    pub height: u32,
    pub prev: BlockHash,
    pub fk: Fk,
}

pub(crate) fn apply_published_header_rows(st: &mut IbdWorkState, rows: &[HeaderPathRow]) {
    for row in rows {
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

fn publish_new_rows(
    published: &ArcSwap<Vec<HeaderPathRow>>,
    st: &IbdWorkState,
    before: &HashSet<BlockHash>,
) {
    let mut rows = Vec::new();
    for hash in &st.ordered {
        if before.contains(hash) {
            continue;
        }
        let Some(height) = st.hash_height.get(hash).copied() else {
            continue;
        };
        let Some(prev) = st.path_prev.get(hash).copied() else {
            continue;
        };
        let Some(fk) = st.header_fks.get(hash).copied() else {
            continue;
        };
        rows.push(HeaderPathRow {
            hash: *hash,
            height,
            prev,
            fk,
        });
    }
    if rows.is_empty() {
        return;
    }
    published.rcu(|cur| {
        let mut next = (**cur).clone();
        next.extend(rows.iter().copied());
        Arc::new(next)
    });
}

pub(crate) fn spawn_header_thread(
    shared: Arc<Mutex<IbdWorkState>>,
    hub: Arc<ChainHub>,
    mut rx: UnboundedReceiver<HeaderCmd>,
    published: Arc<ArcSwap<Vec<HeaderPathRow>>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                HeaderCmd::Batch { peer, headers } => {
                    let fks = hub.ensure_headers_batch(&headers).unwrap_or_default();
                    let work = hub.chain_work().ok();
                    if fks.len() == headers.len() {
                        set_prepared_batch(fks, work);
                    }
                    set_apply_inline(true);
                    {
                        let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        let before: HashSet<BlockHash> = st.ordered.iter().copied().collect();
                        apply_headers_event(&mut st, hub.as_ref(), peer, headers);
                        publish_new_rows(&published, &st, &before);
                    }
                    set_apply_inline(false);
                    clear_prepared_batch();
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
                        let _ = send_getheaders(&mut st, hub.as_ref());
                    }
                    set_warm_locator(None);
                }
                HeaderCmd::Reseed => {
                    let exclude: Vec<[u8; 32]> = {
                        let st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        st.reorg.invalid.iter().collect()
                    };
                    let path = load_resume_entries(hub.as_ref(), &exclude);
                    let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                    let before: HashSet<BlockHash> = st.ordered.iter().copied().collect();
                    install_resumed_path(&mut st, hub.as_ref(), &path);
                    publish_new_rows(&published, &st, &before);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::super::assign::tests::dummy_slot;
    use super::super::state::{IbdWorkState, InflightReq};
    use super::{apply_published_and_stall, HeaderPathRow};
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
}
