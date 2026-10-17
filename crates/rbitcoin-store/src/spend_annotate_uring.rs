//! Completion-driven io_uring RMW for Class A spender-meta annotation.
//!
//! Unknown meta reads a 4 KiB page, decides sole / multi / promote for every
//! slot on that page, then writes the page. Known meta pokes prepared 8-byte
//! slots into the same page RMW. Queue depth stays 128 pages. One in-flight
//! RMW per page. Falls back to the caller on uring setup failure.

use crate::compact::output_flags;
use crate::error::StoreError;
use crate::io_handle::IoHandle;
use crate::spender_table::SpenderTable;
use crate::tx_table::{decode_spent_slot, encode_spent_slot, OutputRecord, TxTable};
use crate::uring_session::{self, UringSession};
use rbitcoin_primitives::Fk;
use std::collections::VecDeque;

const META_LEN: usize = OutputRecord::SPENT_SLOT_LEN;
const MAX_SLOTS: usize = 128;

#[cfg(test)]
thread_local! {
    static SPEND_ANNOTATE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SPEND_ANNOTATE_WRITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn note_spend_read_for_test() {
    #[cfg(test)]
    SPEND_ANNOTATE_READS.with(|n| n.set(n.get() + 1));
}

pub(crate) fn note_spend_write_for_test() {
    #[cfg(test)]
    SPEND_ANNOTATE_WRITES.with(|n| n.set(n.get() + 1));
}
type SpentAbsWrite = (u64, Fk, u32, Fk, u32, [u8; META_LEN]);

enum Phase {
    Reading,
    Writing,
}

/// Annotate spends at absolute meta offsets via io_uring RMW.
///
/// `edges`: `(abs_off, create_tx_fk, vout, spending_tx_fk)`.
/// Returns edges that could not be annotated here (OOB abs, IO error deferred
/// as cold — empty when all succeed). On hard errors returns `Err`.
pub fn put_spend_batch_by_abs_meta_uring(
    txs: &TxTable,
    spenders: &SpenderTable,
    edges: &[(u64, Fk, u32, Fk, u32)],
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    if edges.is_empty() {
        return Ok(Vec::new());
    }
    for &(_, _, _, sfk, _) in edges {
        if sfk.is_null() {
            return Err(StoreError::InvalidFk);
        }
    }

    let body_fd: IoHandle = txs.spent.body_read_fd();
    let body_path = txs.spent.body_file_path().to_path_buf();
    let body_pub = txs.spent.body_published_len();

    let mut cold: Vec<(Fk, u32, Fk, u32)> = Vec::new();
    let mut work: Vec<(u64, Fk, u32, Fk, u32)> = Vec::with_capacity(edges.len());
    for &(abs, cfk, vout, sfk, vin) in edges {
        if abs.saturating_add(META_LEN as u64) > body_pub {
            cold.push((cfk, vout, sfk, vin));
        } else {
            work.push((abs, cfk, vout, sfk, vin));
        }
    }
    if work.is_empty() {
        return Ok(cold);
    }

    let groups = group_edges_by_spent_page(&work, body_pub);
    let extra = annotate_unknown_pages_uring(spenders, &groups, body_fd, &body_path)?;
    cold.extend(extra);
    Ok(cold)
}

struct EdgePage {
    off: u64,
    len: usize,
    edges: Vec<(u64, Fk, u32, Fk, u32)>,
}

fn group_edges_by_spent_page(edges: &[(u64, Fk, u32, Fk, u32)], body_pub: u64) -> Vec<EdgePage> {
    let mut sorted = edges.to_vec();
    sorted.sort_by_key(|e| e.0);
    let mut groups = Vec::new();
    let mut cur_lo = 0u64;
    let mut cur_hi = 0u64;
    let mut cur: Vec<(u64, Fk, u32, Fk, u32)> = Vec::new();
    let flush =
        |groups: &mut Vec<EdgePage>, lo: u64, hi: u64, cur: Vec<(u64, Fk, u32, Fk, u32)>| {
            if cur.is_empty() {
                return;
            }
            let Some((off, len)) = clip_spent_page_window(lo, hi, body_pub) else {
                return;
            };
            groups.push(EdgePage {
                off,
                len,
                edges: cur,
            });
        };
    for e in sorted {
        let (lo, hi) = spent_meta_page_span(e.0);
        if cur.is_empty() {
            cur_lo = lo;
            cur_hi = hi;
            cur.push(e);
            continue;
        }
        if lo < cur_hi {
            cur_hi = cur_hi.max(hi);
            cur.push(e);
        } else {
            flush(&mut groups, cur_lo, cur_hi, std::mem::take(&mut cur));
            cur_lo = lo;
            cur_hi = hi;
            cur.push(e);
        }
    }
    flush(&mut groups, cur_lo, cur_hi, cur);
    groups
}

fn poke_edge_page(
    buf: &mut [u8],
    off: u64,
    edges: &[(u64, Fk, u32, Fk, u32)],
    spenders: &SpenderTable,
    cold: &mut Vec<(Fk, u32, Fk, u32)>,
) -> Result<bool, StoreError> {
    let mut dirty = false;
    for &(abs, cfk, vout, sfk, vin) in edges {
        let i = abs.saturating_sub(off) as usize;
        if i.saturating_add(META_LEN) > buf.len() {
            cold.push((cfk, vout, sfk, vin));
            continue;
        }
        let Ok((flags, field, field_vin)) = decode_spent_slot(&buf[i..i + META_LEN]) else {
            cold.push((cfk, vout, sfk, vin));
            continue;
        };
        match decide_annotate(field, flags, field_vin, sfk, vin, spenders)? {
            AnnotateOp::Skip => {}
            AnnotateOp::Write(meta) => {
                buf[i..i + META_LEN].copy_from_slice(&meta);
                dirty = true;
            }
        }
    }
    Ok(dirty)
}

pub(crate) fn annotate_unknown_pages_pwrite(
    txs: &TxTable,
    spenders: &SpenderTable,
    edges: &[(u64, Fk, u32, Fk, u32)],
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    let body_pub = txs.spent.body_published_len();
    let mut cold = Vec::new();
    let mut live = Vec::new();
    for &e in edges {
        if e.3.is_null() {
            return Err(StoreError::InvalidFk);
        }
        if e.0.saturating_add(META_LEN as u64) > body_pub {
            cold.push((e.1, e.2, e.3, e.4));
        } else {
            live.push(e);
        }
    }
    for g in group_edges_by_spent_page(&live, body_pub) {
        note_spend_read_for_test();
        let mut buf = vec![0u8; g.len];
        if txs
            .spent
            .read_prefix_at_published(body_pub, g.off, g.len as u64, &mut buf)
            .is_err()
        {
            for e in &g.edges {
                cold.push((e.1, e.2, e.3, e.4));
            }
            continue;
        }
        if poke_edge_page(&mut buf, g.off, &g.edges, spenders, &mut cold)? {
            note_spend_write_for_test();
            if txs.spent.write_body_abs(g.off, &buf).is_err() {
                for e in &g.edges {
                    cold.push((e.1, e.2, e.3, e.4));
                }
            }
        }
    }
    Ok(cold)
}

fn annotate_unknown_pages_uring(
    spenders: &SpenderTable,
    groups: &[EdgePage],
    body_fd: IoHandle,
    body_path: &std::path::Path,
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    struct PageSlot {
        group_i: usize,
        phase: Phase,
        buf: Vec<u8>,
        dirty: bool,
    }
    uring_session::with_thread_local(uring_session::DEFAULT_ENTRIES, |session| {
        session.begin_batch()?;
        let epoch = session.epoch();
        let mut pending: VecDeque<usize> = (0..groups.len()).collect();
        let nslots = MAX_SLOTS.min(groups.len().max(1));
        let mut free_slots: Vec<usize> = (0..nslots).collect();
        let mut slots: Vec<Option<PageSlot>> = (0..nslots).map(|_| None).collect();
        let mut session = session.drain_guard();
        let mut in_flight = 0usize;
        let mut cold = Vec::new();

        let arm = |session: &mut UringSession,
                   free_slots: &mut Vec<usize>,
                   slots: &mut [Option<PageSlot>],
                   pending: &mut VecDeque<usize>,
                   groups: &[EdgePage],
                   in_flight: &mut usize,
                   body_fd: IoHandle|
         -> Result<(), StoreError> {
            while *in_flight < slots.len() && session.free_sq() > 0 && !free_slots.is_empty() {
                let Some(gi) = pending.pop_front() else {
                    break;
                };
                let slot = free_slots.pop().unwrap();
                slots[slot] = Some(PageSlot {
                    group_i: gi,
                    phase: Phase::Reading,
                    buf: vec![0u8; groups[gi].len],
                    dirty: false,
                });
                {
                    let s = slots[slot].as_mut().unwrap();
                    let ud = uring_session::pack_ud(
                        uring_session::KIND_SPEND_PAGE_READ,
                        epoch,
                        slot as u32,
                    );
                    note_spend_read_for_test();
                    // SAFETY: `s.buf` lives until this page-read wave drains.
                    unsafe { session.push_pread(body_fd, groups[gi].off, &mut s.buf, ud) }?;
                }
                *in_flight += 1;
            }
            Ok(())
        };

        arm(
            &mut session,
            &mut free_slots,
            &mut slots,
            &mut pending,
            groups,
            &mut in_flight,
            body_fd,
        )?;
        session.sync_submission();
        let _ = session.submit();

        while in_flight > 0 {
            let cqes = session.harvest_ready()?;
            if cqes.is_empty() {
                session.submit_and_wait_one()?;
                continue;
            }
            for (ud, res) in cqes {
                let (kind, _ep, slot) = uring_session::unpack_ud(ud);
                let slot = slot as usize;
                if slot >= slots.len() {
                    return Err(StoreError::Corrupt("spend annotate bad user_data"));
                }
                let mut st = slots[slot]
                    .take()
                    .ok_or(StoreError::Corrupt("spend annotate empty slot"))?;
                let expect = match st.phase {
                    Phase::Reading => uring_session::KIND_SPEND_PAGE_READ,
                    Phase::Writing => uring_session::KIND_SPEND_PAGE_WRITE,
                };
                if kind != expect {
                    return Err(StoreError::Corrupt("spend annotate bad user_data"));
                }
                in_flight = in_flight.saturating_sub(1);
                let gi = st.group_i;
                match st.phase {
                    Phase::Reading => {
                        let want = st.buf.len();
                        if let Err(e) = uring_session::require_full_cqe(res, want, body_path) {
                            free_slots.push(slot);
                            return Err(e);
                        }
                        st.dirty = poke_edge_page(
                            &mut st.buf,
                            groups[gi].off,
                            &groups[gi].edges,
                            spenders,
                            &mut cold,
                        )?;
                        if !st.dirty {
                            free_slots.push(slot);
                            continue;
                        }
                        st.phase = Phase::Writing;
                        slots[slot] = Some(st);
                        {
                            let s = slots[slot].as_mut().unwrap();
                            let ud = uring_session::pack_ud(
                                uring_session::KIND_SPEND_PAGE_WRITE,
                                epoch,
                                slot as u32,
                            );
                            note_spend_write_for_test();
                            // SAFETY: `s.buf` lives until this page-write wave drains.
                            unsafe { session.push_pwrite(body_fd, groups[gi].off, &s.buf, ud) }?;
                        }
                        in_flight += 1;
                    }
                    Phase::Writing => {
                        let want = st.buf.len();
                        if let Err(e) = uring_session::require_full_cqe(res, want, body_path) {
                            free_slots.push(slot);
                            return Err(e);
                        }
                        free_slots.push(slot);
                    }
                }
            }
            arm(
                &mut session,
                &mut free_slots,
                &mut slots,
                &mut pending,
                groups,
                &mut in_flight,
                body_fd,
            )?;
            session.sync_submission();
            let _ = session.submit();
        }
        while let Some(gi) = pending.pop_front() {
            for e in &groups[gi].edges {
                cold.push((e.1, e.2, e.3, e.4));
            }
        }
        Ok(cold)
    })?
}

/// Resolve pure-write annotate backend from env hierarchy.
#[inline]
pub fn spend_ann_backend() -> crate::io_backend::WriteIoBackend {
    crate::io_backend::write_io_backend()
}

/// Decision from structural snapshot + spend_fk (no body read).
enum AnnotateOp {
    Skip,
    Write([u8; META_LEN]),
}

fn decide_annotate(
    field: Fk,
    flags: u8,
    field_vin: u32,
    spend_fk: Fk,
    spend_vin: u32,
    spenders: &SpenderTable,
) -> Result<AnnotateOp, StoreError> {
    let multi = flags & output_flags::MULTI_SPENDER != 0;
    if !multi && field.is_null() {
        let meta = encode_spent_slot(flags & !output_flags::MULTI_SPENDER, spend_fk, spend_vin)?;
        return Ok(AnnotateOp::Write(meta));
    }
    if !multi && field == spend_fk && field_vin == spend_vin {
        return Ok(AnnotateOp::Skip);
    }
    if !multi {
        let e1 = spenders.append(field, field_vin, Fk::NULL)?;
        let e2 = spenders.append(spend_fk, spend_vin, e1)?;
        let meta = encode_spent_slot(flags | output_flags::MULTI_SPENDER, e2, 0)?;
        return Ok(AnnotateOp::Write(meta));
    }
    if multi_list_contains(spenders, field, spend_fk, spend_vin)? {
        return Ok(AnnotateOp::Skip);
    }
    let e = spenders.append(spend_fk, spend_vin, field)?;
    let meta = encode_spent_slot(flags | output_flags::MULTI_SPENDER, e, 0)?;
    Ok(AnnotateOp::Write(meta))
}

fn multi_list_contains(
    spenders: &SpenderTable,
    head: Fk,
    spend_fk: Fk,
    spend_vin: u32,
) -> Result<bool, StoreError> {
    let mut cur = head;
    let mut n = 0u32;
    while let Some(id) = cur.get() {
        n = n.saturating_add(1);
        if n > 1_000_000 {
            return Err(StoreError::Corrupt("invariant: spender multi-list cycle"));
        }
        let (sfk, vin, next) = spenders.get(Fk(id))?;
        if sfk == spend_fk && vin == spend_vin {
            return Ok(true);
        }
        cur = next;
    }
    Ok(false)
}

/// One RMW window on `spent.body` (usually one 4 KiB page, clipped to the
/// published range and past the 16-byte file header).
struct SpentPageGroup {
    off: u64,
    len: usize,
    writes: Vec<SpentAbsWrite>,
}

/// Page span covering the 8-byte slot at `abs` (`[lo, hi)`).
#[inline]
fn spent_meta_page_span(abs: u64) -> (u64, u64) {
    let page = crate::tx_table::BODY_PAGE_SIZE;
    let lo = abs & !(page - 1);
    let last = abs.saturating_add(META_LEN as u64 - 1);
    let hi = (last & !(page - 1)).saturating_add(page);
    (lo, hi)
}

#[inline]
fn clip_spent_page_window(span_lo: u64, span_hi: u64, body_pub: u64) -> Option<(u64, usize)> {
    let off = span_lo.max(crate::file::FILE_HEADER_LEN as u64);
    let end = span_hi.min(body_pub);
    if end <= off {
        return None;
    }
    Some((off, (end - off) as usize))
}

/// Group abs-sorted writes into non-overlapping `spent.body` page spans.
///
/// Same-page slots share one RMW. An 8 B slot that straddles a page boundary
/// extends the span so the next page's writes merge (no overlapping in-flight
/// RMWs). Adjacent pages without a straddle stay separate.
fn group_writes_by_spent_page(writes: &[SpentAbsWrite], body_pub: u64) -> Vec<SpentPageGroup> {
    let mut groups = Vec::new();
    let mut cur_lo = 0u64;
    let mut cur_hi = 0u64;
    let mut cur: Vec<SpentAbsWrite> = Vec::new();

    let flush = |groups: &mut Vec<SpentPageGroup>, lo: u64, hi: u64, cur: Vec<SpentAbsWrite>| {
        if cur.is_empty() {
            return;
        }
        let Some((off, len)) = clip_spent_page_window(lo, hi, body_pub) else {
            return;
        };
        groups.push(SpentPageGroup {
            off,
            len,
            writes: cur,
        });
    };

    for &w in writes {
        let (lo, hi) = spent_meta_page_span(w.0);
        if cur.is_empty() {
            cur_lo = lo;
            cur_hi = hi;
            cur.push(w);
            continue;
        }
        if lo < cur_hi {
            cur_hi = cur_hi.max(hi);
            cur.push(w);
        } else {
            flush(&mut groups, cur_lo, cur_hi, std::mem::take(&mut cur));
            cur_lo = lo;
            cur_hi = hi;
            cur.push(w);
        }
    }
    flush(&mut groups, cur_lo, cur_hi, cur);
    groups
}

fn poke_spent_page(buf: &mut [u8], off: u64, writes: &[SpentAbsWrite]) -> Result<(), StoreError> {
    for &(abs, _, _, _, _, meta) in writes {
        let i = abs.saturating_sub(off) as usize;
        if i.saturating_add(META_LEN) > buf.len() {
            return Err(StoreError::Corrupt(
                "spend annotate poke outside page window",
            ));
        }
        buf[i..i + META_LEN].copy_from_slice(&meta);
    }
    Ok(())
}

fn cold_group_edges(cold: &mut Vec<(Fk, u32, Fk, u32)>, writes: &[SpentAbsWrite]) {
    for &(_, cfk, vout, sfk, vin, _) in writes {
        cold.push((cfk, vout, sfk, vin));
    }
}

/// Pure-write annotate: `known[i]` is structural `(field, flags)` for `abs_edges[i]`.
///
/// Sorts by abs, then page-RMW on `spent.body` (pread page → poke 8 B slots →
/// pwrite page). Returns OOB edges as cold (caller must hard-fail).
pub fn put_spend_batch_by_abs_meta_known(
    txs: &TxTable,
    spenders: &SpenderTable,
    abs_edges: &[(u64, Fk, u32, Fk, u32)],
    known: &[(Fk, u8, u32)],
    backend: crate::io_backend::WriteIoBackend,
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    if abs_edges.is_empty() {
        return Ok(Vec::new());
    }
    if abs_edges.len() != known.len() {
        return Err(StoreError::Corrupt("spend annotate known length mismatch"));
    }
    for &(_, _, _, sfk, _) in abs_edges {
        if sfk.is_null() {
            return Err(StoreError::InvalidFk);
        }
    }

    let mut order: Vec<usize> = (0..abs_edges.len()).collect();
    order.sort_unstable_by_key(|&i| abs_edges[i].0);

    let body_pub = txs.spent.body_published_len();
    let mut cold: Vec<(Fk, u32, Fk, u32)> = Vec::new();
    let mut writes: Vec<SpentAbsWrite> = Vec::with_capacity(order.len());

    for &i in &order {
        let (abs, cfk, vout, sfk, vin) = abs_edges[i];
        if abs.saturating_add(META_LEN as u64) > body_pub {
            cold.push((cfk, vout, sfk, vin));
            continue;
        }
        let (field, flags, field_vin) = known[i];
        match decide_annotate(field, flags, field_vin, sfk, vin, spenders)? {
            AnnotateOp::Skip => {}
            AnnotateOp::Write(meta) => writes.push((abs, cfk, vout, sfk, vin, meta)),
        }
    }

    if writes.is_empty() {
        return Ok(cold);
    }

    // Random advice for this sweep only. Drop restores NORMAL, including on error.
    let _sweep = txs.spent.advise_random_for_sweep();
    match backend {
        crate::io_backend::WriteIoBackend::Uring => {
            put_spend_batch_pure_write_uring(txs, &writes, cold)
        }
        crate::io_backend::WriteIoBackend::Pwrite => {
            put_spend_batch_pure_write_pwrite(txs, &writes, cold)
        }
    }
}

/// libc page-RMW (pread + pwrite, no ring) for prepared 8-byte metas.
fn put_spend_batch_pure_write_pwrite(
    txs: &TxTable,
    writes: &[SpentAbsWrite],
    mut cold: Vec<(Fk, u32, Fk, u32)>,
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    let body_pub = txs.spent.body_published_len();
    let groups = group_writes_by_spent_page(writes, body_pub);
    for g in &groups {
        let mut buf = vec![0u8; g.len];
        if txs
            .spent
            .read_prefix_at_published(body_pub, g.off, g.len as u64, &mut buf)
            .is_err()
        {
            cold_group_edges(&mut cold, &g.writes);
            continue;
        }
        poke_spent_page(&mut buf, g.off, &g.writes)?;
        if txs.spent.write_body_abs(g.off, &buf).is_err() {
            cold_group_edges(&mut cold, &g.writes);
        }
    }
    Ok(cold)
}

/// io_uring page-RMW (pread page → poke → pwrite page) for prepared 8-byte metas.
fn put_spend_batch_pure_write_uring(
    txs: &TxTable,
    writes: &[SpentAbsWrite],
    cold: Vec<(Fk, u32, Fk, u32)>,
) -> Result<Vec<(Fk, u32, Fk, u32)>, StoreError> {
    if writes.is_empty() {
        return Ok(cold);
    }
    let body_pub = txs.spent.body_published_len();
    let groups = group_writes_by_spent_page(writes, body_pub);
    if groups.is_empty() {
        return Ok(cold);
    }
    let body_fd: IoHandle = txs.spent.body_read_fd();
    let body_path = txs.spent.body_file_path().to_path_buf();

    struct PageSlot {
        group_i: usize,
        phase: Phase,
        buf: Vec<u8>,
    }

    let run = uring_session::with_thread_local(uring_session::DEFAULT_ENTRIES, |session| {
        session.begin_batch()?;
        let epoch = session.epoch();
        let mut pending: VecDeque<usize> = (0..groups.len()).collect();
        let nslots = MAX_SLOTS.min(groups.len().max(1));
        let mut free_slots: Vec<usize> = (0..nslots).collect();
        let mut slots: Vec<Option<PageSlot>> = (0..nslots).map(|_| None).collect();
        let mut session = session.drain_guard();
        let mut in_flight = 0usize;
        let mut cold_local = cold.clone();

        let arm = |session: &mut UringSession,
                   free_slots: &mut Vec<usize>,
                   slots: &mut [Option<PageSlot>],
                   pending: &mut VecDeque<usize>,
                   groups: &[SpentPageGroup],
                   in_flight: &mut usize,
                   body_fd: IoHandle|
         -> Result<(), StoreError> {
            while *in_flight < slots.len() && session.free_sq() > 0 && !free_slots.is_empty() {
                let Some(gi) = pending.pop_front() else {
                    break;
                };
                let slot = free_slots.pop().unwrap();
                slots[slot] = Some(PageSlot {
                    group_i: gi,
                    phase: Phase::Reading,
                    buf: vec![0u8; groups[gi].len],
                });
                {
                    let s = slots[slot].as_mut().unwrap();
                    let ud = uring_session::pack_ud(
                        uring_session::KIND_SPEND_PAGE_READ,
                        epoch,
                        slot as u32,
                    );
                    // SAFETY: `s.buf` lives until this page-read wave drains.
                    unsafe { session.push_pread(body_fd, groups[gi].off, &mut s.buf, ud) }?;
                }
                *in_flight += 1;
            }
            Ok(())
        };

        arm(
            &mut session,
            &mut free_slots,
            &mut slots,
            &mut pending,
            &groups,
            &mut in_flight,
            body_fd,
        )?;
        session.sync_submission();
        let _ = session.submit();

        while in_flight > 0 {
            let cqes = session.harvest_ready()?;
            if cqes.is_empty() {
                session.submit_and_wait_one()?;
                continue;
            }
            for (ud, res) in cqes {
                let (kind, _ep, slot) = uring_session::unpack_ud(ud);
                let slot = slot as usize;
                if slot >= slots.len() {
                    return Err(StoreError::Corrupt("spend pure-write bad user_data"));
                }
                let mut st = slots[slot]
                    .take()
                    .ok_or(StoreError::Corrupt("spend pure-write empty slot"))?;
                let expect = match st.phase {
                    Phase::Reading => uring_session::KIND_SPEND_PAGE_READ,
                    Phase::Writing => uring_session::KIND_SPEND_PAGE_WRITE,
                };
                if kind != expect {
                    return Err(StoreError::Corrupt("spend pure-write bad user_data"));
                }
                in_flight = in_flight.saturating_sub(1);
                let gi = st.group_i;
                match st.phase {
                    Phase::Reading => {
                        let want = st.buf.len();
                        if let Err(e) = uring_session::require_full_cqe(res, want, &body_path) {
                            free_slots.push(slot);
                            return Err(e);
                        }
                        poke_spent_page(&mut st.buf, groups[gi].off, &groups[gi].writes)?;
                        st.phase = Phase::Writing;
                        slots[slot] = Some(st);
                        {
                            let s = slots[slot].as_mut().unwrap();
                            let ud = uring_session::pack_ud(
                                uring_session::KIND_SPEND_PAGE_WRITE,
                                epoch,
                                slot as u32,
                            );
                            // SAFETY: `s.buf` lives until this page-write wave drains.
                            unsafe { session.push_pwrite(body_fd, groups[gi].off, &s.buf, ud) }?;
                        }
                        in_flight += 1;
                    }
                    Phase::Writing => {
                        let want = st.buf.len();
                        if let Err(e) = uring_session::require_full_cqe(res, want, &body_path) {
                            free_slots.push(slot);
                            return Err(e);
                        }
                        free_slots.push(slot);
                    }
                }
            }
            arm(
                &mut session,
                &mut free_slots,
                &mut slots,
                &mut pending,
                &groups,
                &mut in_flight,
                body_fd,
            )?;
            session.sync_submission();
            let _ = session.submit();
        }

        while let Some(gi) = pending.pop_front() {
            cold_group_edges(&mut cold_local, &groups[gi].writes);
        }
        Ok(cold_local)
    });
    match run {
        Ok(Ok(c)) => Ok(c),
        Ok(Err(e)) => {
            rbitcoin_log::debug!(
                "store: spend annotate pure-write uring error ({e}); pwrite fallback"
            );
            put_spend_batch_pure_write_pwrite(txs, writes, cold)
        }
        Err(e) => {
            rbitcoin_log::debug!(
                "store: spend annotate pure-write uring unavailable ({e}); pwrite fallback"
            );
            put_spend_batch_pure_write_pwrite(txs, writes, cold)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx_table::{InputRecord, OutputRecord, TxRecord, TxTable};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_table() -> (std::path::PathBuf, TxTable, SpenderTable) {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rbitcoin-ann-known-{id}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let t = TxTable::create_tiny(&dir).unwrap();
        let s = SpenderTable::create(&dir).unwrap();
        (dir, t, s)
    }

    fn put_n_outs(t: &TxTable, n_out: u32, txid_byte: u8) -> (Fk, u64, u64) {
        let tx = TxRecord {
            txid: [txid_byte; 32],
            version: 1,
            locktime: 0,
            input_start_fk: Fk::NULL,
            input_count: 1,
            output_start_fk: Fk::NULL,
            output_count: n_out,
        };
        let inputs = vec![InputRecord {
            prev_txid: [0u8; 32],
            parent: crate::InputParent::Coinbase,
            sequence: u32::MAX,
            script_sig: vec![0x01],
            witness: vec![],
        }];
        let outputs = vec![OutputRecord::unspent(50, vec![0x51]); n_out as usize];
        let fk = t
            .put_full_batch_indexed(&[(tx, inputs, outputs)], false)
            .unwrap()[0];
        let (off, len) = t.spent_range_batch(&[fk]).unwrap()[0].unwrap();
        (fk, off, len)
    }

    fn put_one(t: &TxTable) -> (Fk, u64, u64) {
        put_n_outs(t, 1, 0x11)
    }

    #[test]
    fn pure_write_known_null_mmap_and_uring() {
        for backend in [
            crate::io_backend::WriteIoBackend::Uring,
            crate::io_backend::WriteIoBackend::Pwrite,
        ] {
            let (dir, t, spenders) = temp_table();
            let (cfk, off, _len) = put_one(&t);
            let abs = crate::tx_table::spent_abs(off, 0);
            let bulk = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
            let (field, flags, _vin) = bulk[0].expect("meta");
            assert!(field.is_null());
            let sfk = Fk(99);
            let cold = put_spend_batch_by_abs_meta_known(
                &t,
                &spenders,
                &[(abs, cfk, 0, sfk, 0)],
                &[(field, flags, 0)],
                backend,
            )
            .unwrap();
            assert!(cold.is_empty());
            let bulk2 = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
            let (f2, fl2, _vin) = bulk2[0].unwrap();
            assert_eq!(f2, sfk);
            assert_eq!(fl2 & output_flags::MULTI_SPENDER, 0);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Shipped RMW machine annotates the sole spender on the process session.
    #[test]
    fn shipped_rmw_annotates_sole_spender() {
        let (dir, t, spenders) = temp_table();
        let (cfk, off, _len) = put_one(&t);
        let abs = crate::tx_table::spent_abs(off, 0);
        let sfk = Fk(88);
        let cold = match put_spend_batch_by_abs_meta_uring(&t, &spenders, &[(abs, cfk, 0, sfk, 0)])
        {
            Ok(cold) => cold,
            Err(StoreError::Unavailable) => {
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            Err(e) => panic!("rmw: {e}"),
        };
        assert!(cold.is_empty());
        let bulk = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
        let (field, flags, _vin) = bulk[0].unwrap();
        assert_eq!(field, sfk);
        assert_eq!(flags & output_flags::MULTI_SPENDER, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pure_write_idempotent_skip() {
        let (dir, t, spenders) = temp_table();
        let (cfk, off, len) = put_one(&t);
        let _ = len;
        let abs = crate::tx_table::spent_abs(off, 0);
        let sfk = Fk(77);
        let bulk = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
        let (field, flags, _vin) = bulk[0].unwrap();
        put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, sfk, 0)],
            &[(field, flags, 0)],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        // Second time with known field==sfk → skip
        put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, sfk, 0)],
            &[(sfk, 0, 0)],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        let bulk2 = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
        assert_eq!(bulk2[0].unwrap().0, sfk);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn class_a_same_batch_slot_annotate_is_skip() {
        let (dir, t, spenders) = temp_table();
        let tx = TxRecord {
            txid: [0x21; 32],
            version: 1,
            locktime: 0,
            input_start_fk: Fk::NULL,
            input_count: 1,
            output_start_fk: Fk::NULL,
            output_count: 2,
        };
        let pin = std::sync::Arc::new((
            tx,
            vec![
                OutputRecord::unspent(7, vec![0x51]),
                OutputRecord::unspent(8, vec![0x52]),
            ],
        ));
        let ins = vec![InputRecord::coinbase(u32::MAX, vec![0x01], vec![])];
        let sfk = Fk(9);
        let (fks, _loc) = t
            .put_full_batch_from_pins(&[(pin, ins)], false, &[vec![(0u32, sfk, 0)]])
            .unwrap();
        let cfk = fks[0];
        let (off, _) = t.spent_range(cfk).unwrap();
        let abs = crate::tx_table::spent_abs(off, 0);
        let bulk = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
        assert_eq!(bulk[0].unwrap().0, sfk);
        let cold = put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, sfk, 0)],
            &[(sfk, 0, 0)],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        assert!(cold.is_empty());
        if crate::bulk_io::io_uring_enabled() {
            let cold_u = put_spend_batch_by_abs_meta_known(
                &t,
                &spenders,
                &[(abs, cfk, 0, sfk, 0)],
                &[(sfk, 0, 0)],
                crate::io_backend::WriteIoBackend::Uring,
            )
            .unwrap();
            assert!(cold_u.is_empty());
        }
        let bulk2 = t.get_spender_meta_at_abs_batch(&[abs]).unwrap();
        assert_eq!(bulk2[0].unwrap().0, sfk);
        let abs1 = crate::tx_table::spent_abs(off, 1);
        let neighbor = t.get_spender_meta_at_abs_batch(&[abs1]).unwrap();
        assert!(neighbor[0].unwrap().0.is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pure_write_multi_repeat_same_spend_fk_skips() {
        let (dir, t, spenders) = temp_table();
        let (cfk, off, _len) = put_one(&t);
        let abs = crate::tx_table::spent_abs(off, 0);
        let a = Fk(11);
        let b = Fk(12);
        let known0 = t.get_spender_meta_at_abs_batch(&[abs]).unwrap()[0].unwrap();
        put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, a, 0)],
            &[known0],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        let known1 = t.get_spender_meta_at_abs_batch(&[abs]).unwrap()[0].unwrap();
        put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, b, 0)],
            &[known1],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        let (field, flags, _vin) = t.get_spender_meta_at_abs_batch(&[abs]).unwrap()[0].unwrap();
        assert_ne!(flags & output_flags::MULTI_SPENDER, 0);
        let n0 = spenders.count();
        put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs, cfk, 0, b, 0)],
            &[(field, flags, 0)],
            crate::io_backend::WriteIoBackend::Pwrite,
        )
        .unwrap();
        assert_eq!(spenders.count(), n0, "repeat must not append a list node");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two vouts on one page: both land; a third slot on that page is unchanged.
    #[test]
    fn pure_write_two_vouts_same_page_preserves_neighbor() {
        for backend in [
            crate::io_backend::WriteIoBackend::Uring,
            crate::io_backend::WriteIoBackend::Pwrite,
        ] {
            let (dir, t, spenders) = temp_table();
            let (cfk, off, _len) = put_n_outs(&t, 3, 0x22);
            let abs0 = crate::tx_table::spent_abs(off, 0);
            let abs1 = crate::tx_table::spent_abs(off, 1);
            let abs2 = crate::tx_table::spent_abs(off, 2);
            assert_eq!(
                abs0 & !0xfff,
                abs2 & !0xfff,
                "fixture must keep vout 0 and 2 on one 4 KiB page"
            );

            let sentinel = Fk(0x51);
            let known1 = t.get_spender_meta_at_abs_batch(&[abs1]).unwrap()[0].unwrap();
            let cold_s = put_spend_batch_by_abs_meta_known(
                &t,
                &spenders,
                &[(abs1, cfk, 1, sentinel, 0)],
                &[known1],
                backend,
            )
            .unwrap();
            assert!(cold_s.is_empty());

            let known0 = t.get_spender_meta_at_abs_batch(&[abs0]).unwrap()[0].unwrap();
            let known2 = t.get_spender_meta_at_abs_batch(&[abs2]).unwrap()[0].unwrap();
            let mut hdr_before = [0u8; crate::file::FILE_HEADER_LEN];
            let body_pub = t.spent.body_published_len();
            t.spent
                .read_prefix_at_published(
                    body_pub,
                    0,
                    crate::file::FILE_HEADER_LEN as u64,
                    &mut hdr_before,
                )
                .unwrap();
            let cold = put_spend_batch_by_abs_meta_known(
                &t,
                &spenders,
                &[(abs0, cfk, 0, Fk(10), 0), (abs2, cfk, 2, Fk(12), 0)],
                &[known0, known2],
                backend,
            )
            .unwrap();
            assert!(cold.is_empty());

            let bulk = t
                .get_spender_meta_at_abs_batch(&[abs0, abs1, abs2])
                .unwrap();
            assert_eq!(bulk[0].unwrap().0, Fk(10));
            assert_eq!(
                bulk[1].unwrap().0,
                sentinel,
                "neighbor slot must be preserved"
            );
            assert_eq!(bulk[2].unwrap().0, Fk(12));

            let mut hdr_after = [0u8; crate::file::FILE_HEADER_LEN];
            t.spent
                .read_prefix_at_published(
                    body_pub,
                    0,
                    crate::file::FILE_HEADER_LEN as u64,
                    &mut hdr_after,
                )
                .unwrap();
            assert_eq!(
                hdr_before, hdr_after,
                "page 0 RMW must not rewrite file header"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Vouts whose abs straddle a 4 KiB page both persist.
    #[test]
    fn pure_write_two_pages_both_land() {
        for backend in [
            crate::io_backend::WriteIoBackend::Uring,
            crate::io_backend::WriteIoBackend::Pwrite,
        ] {
            let (dir, t, spenders) = temp_table();
            // 512 × 8 B from file offset 16 crosses 4096 (slot 510 starts at 4096).
            let (cfk, off, _len) = put_n_outs(&t, 512, 0x33);
            let abs0 = crate::tx_table::spent_abs(off, 0);
            let abs_last = crate::tx_table::spent_abs(off, 511);
            assert_ne!(
                abs0 & !0xfff,
                abs_last & !0xfff,
                "fixture must straddle a 4 KiB page"
            );
            let k0 = t.get_spender_meta_at_abs_batch(&[abs0]).unwrap()[0].unwrap();
            let k1 = t.get_spender_meta_at_abs_batch(&[abs_last]).unwrap()[0].unwrap();
            let cold = put_spend_batch_by_abs_meta_known(
                &t,
                &spenders,
                &[(abs0, cfk, 0, Fk(1), 0), (abs_last, cfk, 511, Fk(2), 0)],
                &[k0, k1],
                backend,
            )
            .unwrap();
            assert!(cold.is_empty());
            let bulk = t.get_spender_meta_at_abs_batch(&[abs0, abs_last]).unwrap();
            assert_eq!(bulk[0].unwrap().0, Fk(1));
            assert_eq!(bulk[1].unwrap().0, Fk(2));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Same-page pair is one page-window write, not two 9 B pwrites.
    #[test]
    fn pure_write_same_page_is_one_page_write() {
        if !crate::bulk_io::io_uring_enabled() {
            return;
        }
        let (dir, t, spenders) = temp_table();
        let (cfk, off, _len) = put_n_outs(&t, 3, 0x44);
        let abs0 = crate::tx_table::spent_abs(off, 0);
        let abs2 = crate::tx_table::spent_abs(off, 2);
        let k0 = t.get_spender_meta_at_abs_batch(&[abs0]).unwrap()[0].unwrap();
        let k2 = t.get_spender_meta_at_abs_batch(&[abs2]).unwrap()[0].unwrap();
        let cold = put_spend_batch_by_abs_meta_known(
            &t,
            &spenders,
            &[(abs0, cfk, 0, Fk(3), 0), (abs2, cfk, 2, Fk(4), 0)],
            &[k0, k2],
            crate::io_backend::WriteIoBackend::Uring,
        )
        .unwrap();
        assert!(cold.is_empty());
        let bulk = t.get_spender_meta_at_abs_batch(&[abs0, abs2]).unwrap();
        assert_eq!(bulk[0].unwrap().0, Fk(3));
        assert_eq!(bulk[1].unwrap().0, Fk(4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_meta_annotate_is_one_rmw_per_page() {
        let (dir, t, spenders) = temp_table();
        let (fk, off, _len) = put_n_outs(&t, 520, 0x44);
        let a0 = crate::spent_abs(off, 0);
        let a1 = crate::spent_abs(off, 1);
        let a2 = crate::spent_abs(off, 510);
        assert_eq!(a0 / 4096, a1 / 4096);
        assert_ne!(a0 / 4096, a2 / 4096);
        SPEND_ANNOTATE_READS.with(|n| n.set(0));
        SPEND_ANNOTATE_WRITES.with(|n| n.set(0));
        let edges = [
            (a0, fk, 0u32, Fk(10), 1u32),
            (a1, fk, 1u32, Fk(11), 2u32),
            (a2, fk, 510u32, Fk(12), 3u32),
        ];
        assert!(t
            .put_spend_batch_by_abs_meta(&spenders, &edges)
            .unwrap()
            .is_empty());
        assert_eq!(SPEND_ANNOTATE_READS.with(|n| n.get()), 2);
        assert_eq!(SPEND_ANNOTATE_WRITES.with(|n| n.get()), 2);
        let (off, len) = t.spent_range(fk).unwrap();
        let (m0, f0, v0) = t.get_output_spender_meta_at(off, len, 0).unwrap();
        let (m1, f1, v1) = t.get_output_spender_meta_at(off, len, 1).unwrap();
        let (m2, f2, v2) = t.get_output_spender_meta_at(off, len, 510).unwrap();
        assert!(!m0 && f0 == Fk(10) && v0 == 1);
        assert!(!m1 && f1 == Fk(11) && v1 == 2);
        assert!(!m2 && f2 == Fk(12) && v2 == 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn group_writes_by_spent_page_merges_same_page() {
        let meta = [1u8; META_LEN];
        let slot = META_LEN as u64;
        let writes = [
            (16u64, Fk(1), 0, Fk(10), 0, meta),
            (16 + 2 * slot, Fk(1), 2, Fk(12), 0, meta),
        ];
        let rec = 3 * slot;
        let g = group_writes_by_spent_page(&writes, 16 + rec);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].off, crate::file::FILE_HEADER_LEN as u64);
        assert_eq!(g[0].len, rec as usize);
        assert_eq!(g[0].writes.len(), 2);
    }

    #[test]
    fn group_writes_by_spent_page_keeps_distinct_pages() {
        let meta = [1u8; META_LEN];
        let writes = [
            (16u64, Fk(1), 0, Fk(10), 0, meta),
            (4096u64 + 16, Fk(2), 0, Fk(12), 0, meta),
        ];
        let g = group_writes_by_spent_page(&writes, 4096 + 16 + META_LEN as u64);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].off, crate::file::FILE_HEADER_LEN as u64);
        assert_eq!(g[1].off, 4096);
    }

    #[test]
    fn group_writes_by_spent_page_merges_straddle() {
        let meta = [1u8; META_LEN];
        let slot = META_LEN as u64;
        // Slot starts 3 bytes before the 4 KiB page end so the RMW straddles.
        let first = 4096u64 - 3;
        let second = first + slot;
        let end = second + slot;
        let writes = [
            (first, Fk(1), 0, Fk(10), 0, meta),
            (second, Fk(1), 1, Fk(11), 0, meta),
        ];
        let g = group_writes_by_spent_page(&writes, end);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].writes.len(), 2);
        assert!(g[0].off <= first);
        assert!(g[0].off + g[0].len as u64 >= end);
    }
}
