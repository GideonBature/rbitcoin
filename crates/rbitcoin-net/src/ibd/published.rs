//! Header thread. It persists batches, loads the walk origin, and reseeds.
//! The coordination loop only queues those commands.

use super::dial::set_warm_locator;
use super::events::{
    apply_headers_event, clear_prepared_batch, prepare_header_store, set_apply_inline,
    set_prepared_batch,
};
use super::header_walk::{install_origin, load_origin, origin_input, send_getheaders};
use super::path::{install_resumed_path, load_resume_entries, push_reseed, reseed_candidates};
use super::state::{
    set_defer_header_store, take_store_follow, HeaderCmd, IbdWorkState, FOLLOW_ORIGIN,
    FOLLOW_RESEED, FOLLOW_RESUME, FOLLOW_REWIND,
};
use crate::chain::ChainHub;
use bitcoin::BlockHash;
use rbitcoin_primitives::Fk;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::mpsc::Receiver;

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
                    set_apply_inline(true);
                    set_defer_header_store(true);
                    let pairs = {
                        let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
                        let before: HashSet<BlockHash> = st.ordered.iter().copied().collect();
                        apply_headers_event(&mut st, hub.as_ref(), peer, headers);
                        new_header_fks(&st, &before)
                    };
                    set_defer_header_store(false);
                    set_apply_inline(false);
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
                        new_header_fks(&st, &before)
                    };
                    super::peer_io::publish_header_fks(&header_fks, &pairs);
                    finish_deferred(&shared, &hub);
                }
            }
        }
    })
}

/// Store work the locked apply postponed. Each kind runs at most once.
/// A resume can ask for a rewind; that bit is read after the resume commits.
fn finish_deferred(shared: &Arc<Mutex<IbdWorkState>>, hub: &Arc<ChainHub>) {
    let mut bits = take_store_follow();
    if bits & FOLLOW_RESUME != 0 {
        let exclude: Vec<[u8; 32]> = {
            let st = shared.lock().unwrap_or_else(|err| err.into_inner());
            st.reorg.invalid.iter().collect()
        };
        let path = load_resume_entries(hub.as_ref(), &exclude);
        {
            let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
            set_defer_header_store(true);
            install_resumed_path(&mut st, hub.as_ref(), &path);
            set_defer_header_store(false);
        }
        bits |= take_store_follow();
    }
    if bits & FOLLOW_ORIGIN != 0 {
        let input = {
            let st = shared.lock().unwrap_or_else(|err| err.into_inner());
            origin_input(&st, hub.as_ref())
        };
        if !input.already {
            let loaded = load_origin(hub.as_ref(), &input);
            let mut st = shared.lock().unwrap_or_else(|err| err.into_inner());
            install_origin(&mut st, hub.as_ref(), loaded);
        }
        bits |= take_store_follow();
    }
    if bits & FOLLOW_RESEED != 0 {
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
        bits |= take_store_follow();
    }
    if bits & FOLLOW_REWIND != 0 {
        if let Err(e) = super::reorg::rewind_off_coord_lock(shared, hub.as_ref()) {
            rbitcoin_log::warn!("ibd: header-thread rewind failed: {e}");
        }
    }
}
