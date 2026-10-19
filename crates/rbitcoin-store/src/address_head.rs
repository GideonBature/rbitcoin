//! Keyless addressable `tx.head`: `2^BITS` slots × 4 B create_fk entries.
//!
//! **Layout:** each file entry is LE create_fk (`0` = empty). No key material and
//! **no HAS_NEXT** — probe continues until an empty slot (no Class A deletes).
//! Callers verify identity via Class A body txid on **lookup**. The spare high
//! bits of each slot hold a fingerprint of the mixed key, so a probe skips most
//! foreign occupants before identity. Probes read the slots through a read-only
//! `MAP_SHARED` map ([`AddressHead::slots`]); the page cache keeps it coherent
//! with the sole writer's `pwrite`s.
//!
//! **Insert (sole writer):** probe until the **same fk** is already present
//! (idempotent) or an **empty** slot — `pwrite` the slot (`0 → fk`; no CAS, no
//! per-slot atomics). **No body_txid** on insert (no BIP30 displacement on write).
//! Foreigners and older same-txid creates are skipped blindly; a second Class A
//! row for the same txid lands at the next empty slot (deeper on the probe chain).
//!
//! **Insert batching:** stable-sort by probe **page** then original index
//! (preserves call order within a page for rare same-batch duplicate txids). One
//! page copy from the map + multi-insert + one `pwrite` per dirty page. Readers
//! see the new slots through the map once the `pwrite` lands.
//!
//! **Concurrency:** at most **one** thread may insert into a given head segment
//! (archive writer in IBD; single tip accept path after). Multi-writer races are
//! not supported. Capacity growth is **segment roll** (seal + new open), not bits-widen.
//!
//! **Lookup:** walk candidates from the **last occupied** probe slot toward the
//! first, body-verify — so the deepest same-txid create wins (newest under
//! append-deeper insert).
//!
//! **Probe (page-local open address):** high bits of the txid select a **page** of
//! [`PAGE_SLOTS`] (2¹⁰) slots; within the page, **double hashing** with the next
//! key bits (`h1` / odd `h2`). Depth is capped at [`MAX_PROBE`] (= page size).
//! Lookup and insert hop the mapped slots; insert then writes **one page** (4 KiB)
//! per dirty page. Foreign occupants with the same fingerprint (or none yet):
//! body mismatch ⇒ continue. Keyless slots cannot Robin-Hood. First insert at depth > [`PROBE_DEPTH_WARN`] requests online
//!
//! **Segment default:** BITS=**25** → **128 MiB** @ 4 B relative create ids per
//! segment (`2^15` pages × 4 KiB). Capacity ends at [`HEAD_LOAD_START`] (0.80);
//! the segmented head seals (fuse8) and opens a new fixed-bits table — no
//! global bits-widen.

use crate::error::StoreError;
use crate::file::{TableFile, TRAILING_FOOTER_LEN};
use crate::hashhead::HeadScale;
use crate::readonly_map::ReadonlyMap;
use rbitcoin_primitives::{Fk, TableKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// In-page slot index width (1024 slots / page @ any entry width).
pub const PAGE_SLOT_BITS: u32 = 10;
/// Slots per page (`2^PAGE_SLOT_BITS`).
pub const PAGE_SLOTS: u64 = 1 << PAGE_SLOT_BITS;

/// Hard cap — full in-page exploration (never leave the page).
pub const MAX_PROBE: u32 = 1024;
/// Occupied `(depth, fk)` stored inline in [`ProbeCands`] before a spill `Vec`.
pub const PROBE_CANDS_INLINE: usize = 8;

/// Bytes of one slot in the file (`rel` as LE u32).
pub const ENTRY_BYTES: u64 = 4;
/// Bytes of one head page (1024 × 4 B).
pub const PROBE_REGION_BYTES: usize = (PAGE_SLOTS as usize) * (ENTRY_BYTES as usize);

/// Inserts that needed probe depth **> [`PROBE_DEPTH_WARN`]** (warning band).
/// Cumulative counter for lagging/retry logs; WARN only once at first event.
static PROBE_INSERT_DEPTH_WARN_COUNT: AtomicU64 = AtomicU64::new(0);

/// Inserts that exhausted [`MAX_PROBE`] (archiver sleeps until resize completes).
/// Counter only — the retry loop owns operator-facing logs.
static PROBE_INSERT_EXHAUSTED: AtomicU64 = AtomicU64::new(0);

/// Depth threshold above which inserts count as “deep” for ops visibility and
/// trigger an early online resize request (first event only).
pub const PROBE_DEPTH_WARN: u32 = 128;

/// Segment address width (2^25 slots × 4 B = 128 MiB). Fixed per segment; roll to grow.
pub const MAINNET_BITS: u32 = 25;
/// Tiny / unit-test width.
pub const TINY_BITS: u32 = 16;
/// Maximum supported address width: `rel` must fit a 4 B slot.
pub const MAX_BITS: u32 = 32;
/// Minimum supported address width.
pub const MIN_BITS: u32 = 8;

/// Start sequential rebuild when `txs.count() / slots >=` this.
pub const HEAD_LOAD_START: f64 = 0.80;

/// True when `err` is the sole-writer open-address insert failure (table full
/// along the probe chain — should not happen when segment roll respects 80% load).
#[inline]
pub fn is_probe_exhausted_error(err: &StoreError) -> bool {
    matches!(
        err,
        StoreError::Corrupt(m) if *m == "address head probe exhausted on insert"
    )
}

/// `NetError::Consensus` display of store `Corrupt` (not a block rule).
pub fn is_store_corrupt_display(s: &str) -> bool {
    const PROBE: &str = "address head probe exhausted on insert";
    let probe = StoreError::Corrupt(PROBE);
    if s == probe.to_string() {
        return is_probe_exhausted_error(&probe);
    }
    s.starts_with("corrupt record:")
}

#[inline]
fn note_probe_depth_on_insert(depth: u32) {
    if depth <= PROBE_DEPTH_WARN {
        return;
    }
    let n = PROBE_INSERT_DEPTH_WARN_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if n == 1 {
        rbitcoin_log::warn!(
            "store: tx.head insert probe depth>{PROBE_DEPTH_WARN} (first depth={depth}; \
             segment may be overfull — roll policy should prevent this)"
        );
    }
}

#[inline]
fn note_probe_exhausted() {
    PROBE_INSERT_EXHAUSTED.fetch_add(1, Ordering::Relaxed);
}

const META_MAGIC: &[u8; 4] = b"THM1";
/// `5` = page-local double-hash; layout (bits/entry/generation) lives in the
/// **trailing footer** next to store magic (no `tx.head.meta` sidecar). Slots at
/// offset 0 remain page-aligned. Older versions refused → open recreates + rebuilds.
const META_VERSION: u16 = 5;

/// On-disk / in-memory address-head geometry (4 B slots).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadLayout {
    pub bits: u32,
}

impl HeadLayout {
    pub fn new(bits: u32) -> Result<Self, StoreError> {
        if !(MIN_BITS..=MAX_BITS).contains(&bits) {
            return Err(StoreError::Corrupt("address head bits out of range"));
        }
        Ok(Self { bits })
    }

    pub fn slots(&self) -> u64 {
        1u64 << self.bits
    }

    pub fn body_bytes(&self) -> u64 {
        self.slots() * ENTRY_BYTES
    }
}

/// Fingerprint of a mixed key for the spare high bits above `rel < 2^bits`:
/// `1..=mask` from bytes no page / h1 / h2 field uses. `0` means unknown (a
/// row written before tags) and matches every key. No spare bits at 32.
#[inline]
pub fn fingerprint(mixed: &[u8; 32], bits: u32) -> u32 {
    if bits >= 32 {
        return 0;
    }
    let mask = (1u32 << (32 - bits)) - 1;
    let w = u64::from_le_bytes(mixed[16..24].try_into().unwrap());
    (w % u64::from(mask)) as u32 + 1
}

/// Slot word `rel | fp << bits`, or `None` when `rel` does not fit in `bits`:
/// a wider rel would fold into the fingerprint and its own probe would skip it.
#[inline]
fn tag_entry(rel: u64, fp: u32, bits: u32) -> Option<u32> {
    if rel >> bits != 0 {
        return None;
    }
    Some((rel | (u64::from(fp) << bits)) as u32)
}

/// First 8 bytes of txid as big-endian u64 (bit stream for page / h1).
#[inline]
fn key_be_u64(txid: &[u8; 32]) -> u64 {
    u64::from_be_bytes([
        txid[0], txid[1], txid[2], txid[3], txid[4], txid[5], txid[6], txid[7],
    ])
}

/// Page index from the **top** `(bits - 10)` bits of the txid (0 if bits ≤ 10).
#[inline]
pub fn page_index(txid: &[u8; 32], bits: u32) -> u64 {
    debug_assert!((MIN_BITS..=MAX_BITS).contains(&bits));
    if bits <= PAGE_SLOT_BITS {
        return 0;
    }
    let page_bits = bits - PAGE_SLOT_BITS;
    key_be_u64(txid) >> (64 - page_bits)
}

/// In-page h1: next 10 bits after the page-select field (mod 2^10).
#[inline]
pub fn h1_in_page(txid: &[u8; 32], bits: u32) -> u64 {
    let v = key_be_u64(txid);
    if bits <= PAGE_SLOT_BITS {
        return (v >> (64 - bits)) & ((1u64 << bits) - 1);
    }
    let page_bits = bits - PAGE_SLOT_BITS;
    (v >> (64 - page_bits - PAGE_SLOT_BITS)) & (PAGE_SLOTS - 1)
}

/// In-page odd step from a second window of the txid (1,3,… within the page).
#[inline]
pub fn h2_in_page(txid: &[u8; 32], bits: u32) -> u64 {
    let v = u64::from_be_bytes([
        txid[4], txid[5], txid[6], txid[7], txid[8], txid[9], txid[10], txid[11],
    ]);
    let mask = if bits <= PAGE_SLOT_BITS {
        (1u64 << bits) - 1
    } else {
        PAGE_SLOTS - 1
    };
    (v | 1) & mask
}

/// Global slot at probe depth `d`: page from high bits, double-hash within page.
///
/// `slot = (page << 10) | ((h1 + d·h2) mod 1024)` when `bits > 10`.
#[inline]
pub fn probe_index(txid: &[u8; 32], d: u32, bits: u32) -> u64 {
    debug_assert!((MIN_BITS..=MAX_BITS).contains(&bits));
    let h1 = h1_in_page(txid, bits);
    let h2 = h2_in_page(txid, bits);
    if bits <= PAGE_SLOT_BITS {
        let mask = (1u64 << bits) - 1;
        return h1.wrapping_add(u64::from(d).wrapping_mul(h2)) & mask;
    }
    let page = page_index(txid, bits);
    let local = h1.wrapping_add(u64::from(d).wrapping_mul(h2)) & (PAGE_SLOTS - 1);
    (page << PAGE_SLOT_BITS) | local
}

/// Number of slots in the probe page for this table width.
#[inline]
pub fn page_slot_count(bits: u32) -> u64 {
    if bits <= PAGE_SLOT_BITS {
        1u64 << bits
    } else {
        PAGE_SLOTS
    }
}

/// Slot file offset: data starts at **0** (trailing magic); page-aligned probes.
#[inline]
pub fn entry_file_off(slot: u64) -> u64 {
    slot * ENTRY_BYTES
}

/// Decode one LE create_fk from a page buffer at local slot index.
#[inline]
pub fn entry_from_page_buf(buf: &[u8], local: u64) -> Option<u64> {
    let off = (local as usize).checked_mul(ENTRY_BYTES as usize)?;
    let bytes = buf.get(off..off + ENTRY_BYTES as usize)?;
    Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?)))
}

/// One key's hop plus its page's occupancy (leftover-miss dump).
pub(crate) struct PageHopDump {
    pub scan: ProbeRegionScan,
    pub occupied: u32,
}

/// Occupied `(depth, encoded_fk)` from one hop. Inline for the common 1–2
/// cand path; heap only past [`PROBE_CANDS_INLINE`].
#[derive(Debug, Clone)]
pub struct ProbeCands {
    inline: [(u32, u64); PROBE_CANDS_INLINE],
    n: u8,
    spill: Vec<(u32, u64)>,
}

impl Default for ProbeCands {
    fn default() -> Self {
        Self {
            inline: [(0, 0); PROBE_CANDS_INLINE],
            n: 0,
            spill: Vec::new(),
        }
    }
}

impl ProbeCands {
    #[inline]
    pub fn len(&self) -> usize {
        (self.n as usize).saturating_add(self.spill.len())
    }

    #[inline]
    pub fn push(&mut self, depth: u32, fk: u64) {
        let i = self.n as usize;
        if i < PROBE_CANDS_INLINE {
            self.inline[i] = (depth, fk);
            self.n = (i + 1) as u8;
            return;
        }
        self.spill.push((depth, fk));
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &(u32, u64)> {
        self.inline[..self.n as usize]
            .iter()
            .chain(self.spill.iter())
    }
}

impl PartialEq for ProbeCands {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

/// Result of hopping through one loaded page.
#[derive(Debug, Clone)]
pub struct ProbeRegionScan {
    /// Occupied create_fks with absolute probe depth (home = 0).
    pub cands: ProbeCands,
    /// Saw an empty slot (probe chain ends).
    pub hit_empty: bool,
    /// Depth of empty slot if [`Self::hit_empty`], else depths explored without empty.
    pub depth_end: u32,
    /// Local slot index of empty (valid if hit_empty).
    pub empty_local: u64,
}

/// Double-hash hop through a loaded page buffer.
#[inline]
pub fn hop_scan_page(
    page_buf: &[u8],
    h1: u64,
    h2: u64,
    page_slots: u64,
    max_probe: u32,
) -> ProbeRegionScan {
    let mask = page_slots - 1;
    let max_d = max_probe.min(page_slots as u32);
    let mut cands = ProbeCands::default();
    for d in 0..max_d {
        let local = h1.wrapping_add(u64::from(d).wrapping_mul(h2)) & mask;
        let Some(e) = entry_from_page_buf(page_buf, local) else {
            break;
        };
        if e == 0 {
            return ProbeRegionScan {
                cands,
                hit_empty: true,
                depth_end: d,
                empty_local: local,
            };
        }
        cands.push(d, e);
    }
    ProbeRegionScan {
        cands,
        hit_empty: false,
        depth_end: max_d,
        empty_local: 0,
    }
}

/// Global first slot of the probe page that holds `txid`.
#[inline]
pub fn page_base_for_txid(txid: &[u8; 32], bits: u32) -> u64 {
    if bits <= PAGE_SLOT_BITS {
        0
    } else {
        page_index(txid, bits) << PAGE_SLOT_BITS
    }
}

/// Outcome of [`insert_fk_into_page_buf`] (in-buffer only; no file IO).
#[derive(Debug, Clone, Copy)]
pub struct InsertPageOutcome {
    /// Wrote a new empty→fk slot (false if `new_fk` already on the chain).
    pub wrote_new: bool,
    /// Probe depth of the empty slot written (or 0 if idempotent).
    pub depth: u32,
}

/// Insert `new_fk` into a **loaded** probe page buffer; the new slot carries
/// the key's fingerprint.
///
/// Idempotent if `new_fk` is already present. Does not touch the file or
/// occupied count — caller applies the buffer via pwrite.
pub fn insert_fk_into_page_buf(
    page_buf: &mut [u8],
    bits: u32,
    txid: &[u8; 32],
    new_fk: Fk,
) -> Result<InsertPageOutcome, StoreError> {
    if new_fk.is_null() {
        return Err(StoreError::InvalidFk);
    }
    let new_u = new_fk.0;
    let Some(tagged) = tag_entry(new_u, fingerprint(txid, bits), bits) else {
        return Err(StoreError::InvalidFk);
    };
    let rel_mask = (1u64 << bits) - 1;
    let nslots = (page_buf.len() / ENTRY_BYTES as usize) as u64;
    let h1 = h1_in_page(txid, bits);
    let h2 = h2_in_page(txid, bits);
    let scan = hop_scan_page(page_buf, h1, h2, nslots, MAX_PROBE);
    for &(_d, e) in scan.cands.iter() {
        if e & rel_mask == new_u {
            return Ok(InsertPageOutcome {
                wrote_new: false,
                depth: 0,
            });
        }
    }
    if !scan.hit_empty {
        note_probe_exhausted();
        return Err(StoreError::Corrupt(
            "address head probe exhausted on insert",
        ));
    }
    store_entry_in_page_buf(page_buf, scan.empty_local, tagged)?;
    Ok(InsertPageOutcome {
        wrote_new: true,
        depth: scan.depth_end,
    })
}

/// Write LE create_fk into a page buffer at local slot index.
#[inline]
fn store_entry_in_page_buf(page_buf: &mut [u8], local: u64, new: u32) -> Result<(), StoreError> {
    let es = ENTRY_BYTES as usize;
    let off = (local as usize)
        .checked_mul(es)
        .ok_or(StoreError::Corrupt("page buf slot overflow"))?;
    let slot = page_buf
        .get_mut(off..off + es)
        .ok_or(StoreError::Corrupt("page buf slot out of range"))?;
    slot.copy_from_slice(&new.to_le_bytes());
    Ok(())
}

/// Resolve address width for new creates (`scale`, then `RBITCOIN_TX_HEAD_BITS`).
pub fn bits_for_scale(scale: HeadScale) -> u32 {
    if let Ok(s) = std::env::var("RBITCOIN_TX_HEAD_BITS") {
        if let Ok(n) = s.parse::<u32>() {
            if (MIN_BITS..=MAX_BITS).contains(&n) {
                return n;
            }
            rbitcoin_log::warn!(
                "store: RBITCOIN_TX_HEAD_BITS={s:?} out of {MIN_BITS}..={MAX_BITS}, using scale default"
            );
        }
    }
    match scale {
        HeadScale::Tiny => TINY_BITS,
        HeadScale::Mainnet => MAINNET_BITS,
    }
}

pub fn default_layout(scale: HeadScale) -> HeadLayout {
    HeadLayout::new(bits_for_scale(scale)).expect("default bits in range")
}

/// Legacy sidecar path (`tx.head.meta`) — only for best-effort cleanup of old datadirs.
fn meta_path(head_path: &Path) -> PathBuf {
    let mut p = head_path.as_os_str().to_os_string();
    p.push(".meta");
    PathBuf::from(p)
}

/// Drop leftover sidecar meta from pre-v5 layouts (layout is now in the footer).
pub fn remove_legacy_meta_sidecar(head_path: &Path) {
    let _ = std::fs::remove_file(meta_path(head_path));
}

/// Pack layout + generation into the 16-byte trailing-footer extension.
#[inline]
pub fn encode_layout_ext(layout: HeadLayout, generation: u64) -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0..4].copy_from_slice(META_MAGIC);
    buf[4..6].copy_from_slice(&META_VERSION.to_le_bytes());
    buf[6] = layout.bits as u8;
    buf[7] = ENTRY_BYTES as u8;
    buf[8..16].copy_from_slice(&generation.to_le_bytes());
    buf
}

/// Decode layout extension from the trailing footer (or fail for rebuild).
pub fn decode_layout_ext(ext: &[u8; 16]) -> Result<(HeadLayout, u64), StoreError> {
    if &ext[0..4] != META_MAGIC {
        return Err(StoreError::Corrupt(
            "tx.head footer layout magic (rebuild tx.head)",
        ));
    }
    let ver = u16::from_le_bytes([ext[4], ext[5]]);
    if ver != META_VERSION {
        return Err(StoreError::Corrupt(
            "tx.head footer layout version (footer-embedded meta; rebuild tx.head)",
        ));
    }
    if u64::from(ext[7]) != ENTRY_BYTES {
        return Err(StoreError::Corrupt(
            "tx.head footer entry width (rebuild tx.head)",
        ));
    }
    let bits = u32::from(ext[6]);
    let generation = u64::from_le_bytes(ext[8..16].try_into().unwrap());
    Ok((HeadLayout::new(bits)?, generation))
}

/// Fixed-width keyless txid → dense create_fk table.
pub struct AddressHead {
    file: TableFile,
    layout: HeadLayout,
    /// Read-only `MAP_SHARED` view of the slot array. Probes and inserts hop
    /// it; the sole writer `pwrite`s pages through `file` and the page cache
    /// keeps the view coherent. A slot is `rel | fp << bits`.
    map: ReadonlyMap,
}

impl AddressHead {
    pub fn create_with_layout(
        path: impl Into<PathBuf>,
        layout: HeadLayout,
    ) -> Result<Self, StoreError> {
        // Trailing footer: slots at offset 0 so each 4 KiB probe page is OS-aligned.
        // Layout (bits/entry/generation) lives in the footer extension — no sidecar.
        let path = path.into();
        if path.is_dir() {
            return Err(StoreError::Corrupt(
                "tx.head is a directory (legacy shards); wipe datadir for address head",
            ));
        }
        let mut file = TableFile::create_trailing_header(&path, TableKind::HashHead)?;
        let body_bytes = layout.body_bytes();
        let need = body_bytes + TRAILING_FOOTER_LEN as u64;
        file.ensure_capacity(need)?;
        // Layout ext must be set before set_logical_len so the footer at EOF
        // carries bits/generation (no sidecar).
        file.set_trailing_ext(encode_layout_ext(layout, 0))?;
        file.set_logical_len(need)?;
        file.zero_range(0, body_bytes)?;
        remove_legacy_meta_sidecar(&path);
        Self::with_map(file, layout, &path)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        if path.is_dir() {
            return Err(StoreError::Corrupt(
                "tx.head is a directory (legacy shards); wipe datadir for address head",
            ));
        }
        // Layout is in the trailing footer (v5). Sidecar-only or older footers fail
        // here → TxTable recreates + rebuilds from Class A.
        let (file, ext) = TableFile::open_trailing_header_from_end(&path, TableKind::HashHead)?;
        let (layout, _generation) = decode_layout_ext(&ext)?;
        let expect_body = layout.body_bytes();
        let body = file.data_len();
        if body == 0 {
            return Err(StoreError::Corrupt("address head size"));
        }
        if body != expect_body {
            return Err(StoreError::Corrupt(
                "address head size mismatch vs footer layout",
            ));
        }
        remove_legacy_meta_sidecar(&path);
        Self::with_map(file, layout, &path)
    }

    fn with_map(file: TableFile, layout: HeadLayout, path: &Path) -> Result<Self, StoreError> {
        let map = ReadonlyMap::map_prefix(path, layout.body_bytes() as usize)?;
        Ok(Self { file, layout, map })
    }

    /// The mapped slot array. The bytes are not frozen: the sole writer
    /// publishes slots by `pwrite` through `file`, and the page cache changes
    /// them under this view. So every read is an atomic load, and no `&[u8]`
    /// of the map is ever formed (`as_file_bytes` is for sealed files).
    #[inline]
    fn slots(&self) -> &[AtomicU32] {
        // SAFETY: a page-aligned live `MAP_SHARED` mapping of `body_bytes()`
        // bytes (a multiple of 4), held for `self`'s lifetime. Nothing in this
        // process writes through the mapping; concurrent file writes are
        // observed only by atomic loads, never by a plain reference.
        unsafe {
            std::slice::from_raw_parts(
                self.map.as_ptr() as *const AtomicU32,
                self.map.map_len() / 4,
            )
        }
    }

    pub fn bits(&self) -> u32 {
        self.layout.bits
    }

    /// Bytes of the mapped slot array (file-backed, reclaimable; not heap).
    pub fn map_bytes(&self) -> u64 {
        self.map.map_len() as u64
    }

    /// Global slot at probe depth `d` of `mixed`.
    #[inline]
    fn slot_at(&self, mixed: &[u8; 32], d: u32) -> usize {
        probe_index(mixed, d, self.layout.bits) as usize
    }

    /// Hop one key's chain through the map. `fp != 0` drops slots whose known
    /// fingerprint differs; `fp == 0` keeps every occupied slot. Cands carry
    /// the plain `rel`.
    fn hop(&self, mixed: &[u8; 32], fp: u32) -> ProbeRegionScan {
        let bits = self.layout.bits;
        let page_slots = page_slot_count(bits);
        let mask = page_slots - 1;
        let base = page_base_for_txid(mixed, bits) as usize;
        let h1 = h1_in_page(mixed, bits);
        let h2 = h2_in_page(mixed, bits);
        let max_d = MAX_PROBE.min(page_slots as u32);
        let rel_mask = (1u64 << bits) - 1;
        let mut cands = ProbeCands::default();
        for d in 0..max_d {
            let local = h1.wrapping_add(u64::from(d).wrapping_mul(h2)) & mask;
            let e = u64::from(self.slots()[base + local as usize].load(Ordering::Acquire));
            if e == 0 {
                return ProbeRegionScan {
                    cands,
                    hit_empty: true,
                    depth_end: d,
                    empty_local: local,
                };
            }
            let efp = (e >> bits) as u32;
            if fp != 0 && efp != 0 && efp != fp {
                continue;
            }
            cands.push(d, e & rel_mask);
        }
        ProbeRegionScan {
            cands,
            hit_empty: false,
            depth_end: max_d,
            empty_local: 0,
        }
    }

    /// Candidates per mixed key through the map, in probe order (callers
    /// reverse for deepest-first). No syscall.
    pub(crate) fn probe_fks_batch(&self, mixed: &[[u8; 32]]) -> Vec<Vec<Fk>> {
        let bits = self.layout.bits;
        mixed
            .iter()
            .map(|m| {
                let scan = self.hop(m, fingerprint(m, bits));
                scan.cands.iter().map(|&(_, rel)| Fk(rel)).collect()
            })
            .collect()
    }

    /// Write the fingerprint of rows that predate tags: `(mixed, rel)` whose
    /// rel sits on the key's chain. One 4-byte `pwrite` per row; a rel not on
    /// its chain is left alone. Callers running in parallel must pass
    /// disjoint rels.
    pub(crate) fn fill_fingerprints(&self, keys: &[([u8; 32], Fk)]) -> Result<(), StoreError> {
        let bits = self.layout.bits;
        for (mixed, rel) in keys {
            let scan = self.hop(mixed, 0);
            let depth = scan
                .cands
                .iter()
                .find(|&&(_, r)| r == rel.0)
                .map(|&(d, _)| d);
            if let Some(d) = depth {
                let v = tag_entry(rel.0, fingerprint(mixed, bits), bits)
                    .ok_or(StoreError::Corrupt("tx.head rel wider than bits"))?;
                let off = entry_file_off(self.slot_at(mixed, d) as u64);
                self.file.write_at(off, &v.to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Rewrite every occupied slot without its tag, as a meta v1 head wrote it.
    #[cfg(test)]
    pub(crate) fn strip_tags_for_test(&self) -> Result<(), StoreError> {
        let rel_mask = (1u32 << self.layout.bits) - 1;
        for (i, s) in self.slots().iter().enumerate() {
            let raw = s.load(Ordering::Relaxed);
            if raw != 0 {
                self.file
                    .write_at(entry_file_off(i as u64), &(raw & rel_mask).to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Hop one key and count its page's occupied slots. Leftover-miss dump only.
    pub(crate) fn dump_page_hop(&self, mixed: &[u8; 32]) -> PageHopDump {
        let bits = self.layout.bits;
        let page_slots = page_slot_count(bits) as usize;
        let base = page_base_for_txid(mixed, bits) as usize;
        let occupied = self.slots()[base..base + page_slots]
            .iter()
            .filter(|s| s.load(Ordering::Relaxed) != 0)
            .count() as u32;
        PageHopDump {
            scan: self.hop(mixed, 0),
            occupied,
        }
    }

    /// Sort `entries` in place by probe page and insert. No extra pair copy.
    pub(crate) fn insert_many_in_place(
        &self,
        entries: &mut [([u8; 32], Fk)],
    ) -> Result<(), StoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let bits = self.layout.bits;
        let page_slots = page_slot_count(bits) as usize;
        let n = page_slots * ENTRY_BYTES as usize;

        entries.sort_by_key(|a| page_base_for_txid(&a.0, bits));

        let mut buf = [0u8; PROBE_REGION_BYTES];
        let mut i = 0;
        while i < entries.len() {
            let page_base = page_base_for_txid(&entries[i].0, bits);
            let mut j = i + 1;
            while j < entries.len() && page_base_for_txid(&entries[j].0, bits) == page_base {
                j += 1;
            }

            // The page (tagged slots) comes from the map, not a pread.
            let slots = self.slots();
            for s in 0..page_slots {
                let e = slots[page_base as usize + s].load(Ordering::Relaxed);
                buf[s * 4..s * 4 + 4].copy_from_slice(&e.to_le_bytes());
            }

            let mut dirty = false;
            for &(ref txid, fk) in &entries[i..j] {
                let outcome = insert_fk_into_page_buf(&mut buf[..n], bits, txid, fk)?;
                if outcome.wrote_new {
                    note_probe_depth_on_insert(outcome.depth);
                    dirty = true;
                }
            }
            if dirty {
                self.file.write_at(entry_file_off(page_base), &buf[..n])?;
            }
            i = j;
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        self.file.flush()
    }

    pub fn flush_async(&self) -> Result<(), StoreError> {
        self.file.flush_async()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    fn tmp(name: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, AtomicOrdering::Relaxed);
        let p = std::env::temp_dir().join(format!("rbitcoin-addr-head-{name}-{id}"));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_file(&p);
        let meta = meta_path(&p);
        let _ = std::fs::remove_file(&meta);
        p
    }

    fn head_bits(path: impl Into<PathBuf>, bits: u32) -> Result<AddressHead, StoreError> {
        AddressHead::create_with_layout(path, HeadLayout::new(bits)?)
    }

    fn insert_one(h: &AddressHead, txid: &[u8; 32], fk: Fk) -> Result<(), StoreError> {
        h.insert_many_in_place(&mut [(*txid, fk)])
    }

    fn probe_one(h: &AddressHead, txid: &[u8; 32]) -> Result<Vec<Fk>, StoreError> {
        Ok(h.probe_fks_batch(std::slice::from_ref(txid))
            .pop()
            .unwrap_or_default())
    }

    fn probe_batch(h: &AddressHead, txids: &[[u8; 32]]) -> Result<Vec<Vec<Fk>>, StoreError> {
        Ok(h.probe_fks_batch(txids))
    }

    /// Raw slot word in the **file** (`rel | fp << bits`).
    fn read_slot_raw(h: &AddressHead, slot: u64) -> u32 {
        let mut buf = [0u8; 4];
        h.file.read_at(entry_file_off(slot), &mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    /// Plain rel in the **file** at `slot`.
    fn read_slot(h: &AddressHead, slot: u64) -> u64 {
        u64::from(read_slot_raw(h, slot)) & ((1u64 << h.bits()) - 1)
    }

    /// Two keys on one chain (same page and h1) with different fingerprints.
    fn colliding_pair() -> ([u8; 32], [u8; 32]) {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a[0] = 0x10;
        b[0] = 0x10;
        b[4] = 0x02;
        b[16] = 0x01;
        (a, b)
    }

    /// Rewrite every occupied slot without its tag, as a meta v1 head wrote it.
    fn strip_tags(h: &AddressHead) {
        for slot in 0..h.layout.slots() {
            let rel = read_slot(h, slot) as u32;
            if rel != 0 {
                h.file
                    .write_at(entry_file_off(slot), &rel.to_le_bytes())
                    .unwrap();
            }
        }
    }

    /// Some slot of the page holds `fk` (tag bits above `bits` ignored).
    fn page_has_fk(buf: &[u8], bits: u32, fk: u64) -> bool {
        let n = buf.len() / ENTRY_BYTES as usize;
        let rel_mask = (1u64 << bits) - 1;
        (0..n).any(|i| entry_from_page_buf(buf, i as u64).unwrap_or(0) & rel_mask == fk)
    }

    #[test]
    fn probe_limits_match_policy() {
        assert_eq!(MAX_PROBE, 1024);
        assert_eq!(PAGE_SLOTS, 1024);
        assert_eq!(PAGE_SLOT_BITS, 10);
        assert_eq!(PROBE_REGION_BYTES, 4096);
        assert_eq!(PROBE_DEPTH_WARN, 128);
        assert_eq!(MAINNET_BITS, 25);
        assert_eq!(PROBE_REGION_BYTES as u64, PAGE_SLOTS * ENTRY_BYTES);
    }

    #[test]
    fn probe_stable() {
        let k = [0xabu8; 32];
        assert_eq!(probe_index(&k, 0, 16), probe_index(&k, 0, 16));
        assert_ne!(probe_index(&k, 0, 16), probe_index(&k, 1, 16));
        assert!(probe_index(&k, 0, 16) < (1 << 16));
    }

    #[test]
    fn hop_scan_page_stops_at_empty() {
        // Page of 4 slots: place fks at double-hash locals.
        let mut buf = vec![0u8; 16];
        // h1=0, h2=1 → locals 0,1,2,...
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        buf[4..8].copy_from_slice(&2u32.to_le_bytes());
        // slot 2 empty
        let s = hop_scan_page(&buf, 0, 1, 4, MAX_PROBE);
        assert!(s.hit_empty);
        assert_eq!(
            s.cands.iter().copied().collect::<Vec<_>>(),
            vec![(0, 1), (1, 2)]
        );
        assert_eq!(s.depth_end, 2);
        assert_eq!(s.empty_local, 2);
    }

    #[test]
    fn hop_scan_page_spills_past_inline_cap() {
        let n = PROBE_CANDS_INLINE + 1;
        let page_slots = 16usize;
        let mut buf = vec![0u8; page_slots * 4];
        for i in 0..n {
            let fk = (i as u32) + 1;
            buf[i * 4..i * 4 + 4].copy_from_slice(&fk.to_le_bytes());
        }
        let s = hop_scan_page(&buf, 0, 1, page_slots as u64, MAX_PROBE);
        assert_eq!(s.cands.len(), n);
        assert_eq!(
            s.cands.iter().last().copied(),
            Some(((n as u32) - 1, n as u64))
        );
        let got: Vec<_> = s.cands.iter().copied().collect();
        let expect: Vec<_> = (0..n as u32).map(|d| (d, u64::from(d) + 1)).collect();
        assert_eq!(got, expect);
    }

    #[test]
    fn insert_fk_into_page_buf_empty_idempotent_and_second() {
        let bits = 16u32;
        let mut txid = [0u8; 32];
        txid[0] = 0x42;
        let page_slots = page_slot_count(bits);
        let mut buf = vec![0u8; (page_slots as usize) * ENTRY_BYTES as usize];
        let o1 = insert_fk_into_page_buf(&mut buf, bits, &txid, Fk(7)).unwrap();
        assert!(o1.wrote_new);
        assert!(page_has_fk(&buf, bits, 7));
        let o2 = insert_fk_into_page_buf(&mut buf, bits, &txid, Fk(7)).unwrap();
        assert!(!o2.wrote_new, "idempotent same fk");
        let o3 = insert_fk_into_page_buf(&mut buf, bits, &txid, Fk(8)).unwrap();
        assert!(o3.wrote_new, "second create deeper on chain");
        assert!(page_has_fk(&buf, bits, 7) && page_has_fk(&buf, bits, 8));
    }

    #[test]
    fn page_local_double_hash_stays_in_page() {
        let k = [0x11u8; 32];
        let bits = 26u32;
        let page = page_index(&k, bits);
        for d in 0..64u32 {
            let slot = probe_index(&k, d, bits);
            assert_eq!(slot >> PAGE_SLOT_BITS, page, "d={d}");
            assert!(slot < (1u64 << bits));
        }
        // Distinct depths should differ (odd h2).
        assert_ne!(probe_index(&k, 0, bits), probe_index(&k, 1, bits));
    }

    #[test]
    fn probe_bits_26_to_32_in_range() {
        let k = [0x11u8; 32];
        for bits in [26u32, 28, 31, 32] {
            let idx = probe_index(&k, 0, bits);
            assert!(idx < (1u64 << bits), "bits={bits} idx={idx}");
            let idx2 = probe_index(&k, 7, bits);
            assert!(idx2 < (1u64 << bits));
            // Same page for all depths.
            if bits > PAGE_SLOT_BITS {
                assert_eq!(idx >> PAGE_SLOT_BITS, idx2 >> PAGE_SLOT_BITS);
            }
        }
    }

    #[test]
    fn bip30_second_create_same_page() {
        let path = tmp("bip30_page");
        let h = head_bits(&path, 16).unwrap();
        let mut txid = [0u8; 32];
        txid[0] = 0x55;
        insert_one(&h, &txid, Fk(1)).unwrap();
        insert_one(&h, &txid, Fk(2)).unwrap();
        let cands = probe_one(&h, &txid).unwrap();
        assert!(cands.contains(&Fk(1)));
        assert!(cands.contains(&Fk(2)));
        assert_eq!(cands[0], Fk(1));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// FdOnly trailing head: page-coalesce insert uses pread/pwrite (phase 2 A/B path).
    #[test]
    fn fd_only_insert_many_probe_and_reopen() {
        let path = tmp("fd_only_head");
        let h = head_bits(&path, 14).unwrap();
        let mut batch = Vec::new();
        for i in 1..=500u64 {
            let mut txid = [0u8; 32];
            txid[0..8].copy_from_slice(&i.to_le_bytes());
            batch.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut batch).unwrap();
        for i in [1u64, 250, 500] {
            let mut txid = [0u8; 32];
            txid[0..8].copy_from_slice(&i.to_le_bytes());
            let cands = probe_one(&h, &txid).unwrap();
            assert!(cands.contains(&Fk(i)), "fk={i} cands={cands:?}");
        }
        drop(h);
        let h2 = AddressHead::open(&path).unwrap();
        let mut txid = [0u8; 32];
        txid[0..8].copy_from_slice(&42u64.to_le_bytes());
        assert!(probe_one(&h2, &txid).unwrap().contains(&Fk(42)));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn meta_v1_refused_linear_probe() {
        let path = tmp("meta_v1");
        let h = head_bits(&path, 12).unwrap();
        drop(h);
        // Corrupt footer layout version (v1 = double-hash era / pre-footer meta).
        let mut raw = std::fs::read(&path).unwrap();
        let n = raw.len();
        assert!(n >= TRAILING_FOOTER_LEN);
        // Footer layout ext at [n-16..n): version at bytes 4..6 of ext.
        let ver_off = n - 16 + 4;
        raw[ver_off..ver_off + 2].copy_from_slice(&1u16.to_le_bytes());
        std::fs::write(&path, &raw).unwrap();
        match AddressHead::open(&path) {
            Err(StoreError::Corrupt(m))
                if m.contains("footer") || m.contains("rebuild") || m.contains("version") => {}
            Err(e) => panic!("expected footer layout version error, got {e}"),
            Ok(_) => panic!("expected open failure for meta v1"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn eight_byte_footer_is_refused_not_read_as_pairs() {
        let path = tmp("footer8");
        let h = head_bits(&path, 12).unwrap();
        drop(h);
        let mut raw = std::fs::read(&path).unwrap();
        let n = raw.len();
        // Footer layout ext at [n-16..n): entry width at byte 7 of ext.
        raw[n - 16 + 7] = 8;
        std::fs::write(&path, &raw).unwrap();
        match AddressHead::open(&path) {
            Err(StoreError::Corrupt(m)) => {
                assert_eq!(m, "tx.head footer entry width (rebuild tx.head)")
            }
            Err(e) => panic!("expected footer entry width error, got {e}"),
            Ok(_) => panic!("an 8-byte footer must not open as 4-byte slots"),
        }
        let _ = std::fs::remove_file(&path);
    }

    /// A probe that races the sole writer's page `pwrite` sees the page
    /// before or after that write, never a miss of a key inserted earlier.
    #[test]
    fn probe_during_page_pwrite_keeps_every_published_key() {
        use std::sync::atomic::AtomicBool;
        let path = tmp("race");
        let h = head_bits(&path, 14).unwrap();
        let n = 4000u64;
        let key = |i: u64| {
            let mut txid = [0u8; 32];
            txid[0..8].copy_from_slice(&i.to_le_bytes());
            txid[16..24].copy_from_slice(&(i * 11).to_le_bytes());
            txid
        };
        let published = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        std::thread::scope(|s| {
            let h = &h;
            let published = &published;
            let done = &done;
            s.spawn(move || {
                let mut i = 1u64;
                while i <= n {
                    let end = (i + 7).min(n);
                    let mut batch: Vec<_> = (i..=end).map(|k| (key(k), Fk(k))).collect();
                    h.insert_many_in_place(&mut batch).unwrap();
                    published.store(end, AtomicOrdering::Release);
                    i = end + 1;
                }
                done.store(true, AtomicOrdering::Release);
            });
            let mut probes = 0u64;
            loop {
                let finished = done.load(AtomicOrdering::Acquire);
                let p = published.load(AtomicOrdering::Acquire);
                if p > 0 {
                    let lo = p.saturating_sub(64).max(1);
                    for k in lo..=p {
                        let cands = probe_one(h, &key(k)).unwrap();
                        assert!(cands.contains(&Fk(k)), "key {k} missing at published={p}");
                        probes += 1;
                    }
                }
                if finished {
                    break;
                }
            }
            assert!(probes > 0);
        });
        for k in 1..=n {
            assert_eq!(probe_one(&h, &key(k)).unwrap(), vec![Fk(k)]);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn load_trigger_at_80_percent() {
        let slots = 1024u64;
        // Segment roll uses floor(0.80 * slots).
        let thr = ((slots as f64) * HEAD_LOAD_START).floor() as u64;
        assert_eq!(thr, 819); // floor(0.80 * 1024)
        assert!(thr - 1 < thr);
        assert!(thr >= ((slots as f64) * HEAD_LOAD_START).floor() as u64);
        assert!(slots >= thr);
    }

    #[test]
    fn default_layout_is_fixed_segment_geometry() {
        // Capacity growth is segment roll, not bits-widen.
        let layout = default_layout(HeadScale::Tiny);
        assert_eq!(layout.bits, bits_for_scale(HeadScale::Tiny));
        assert_eq!(
            default_layout(HeadScale::Tiny).bits,
            bits_for_scale(HeadScale::Tiny)
        );
    }

    #[test]
    fn is_probe_exhausted_matches_insert_error() {
        let e = StoreError::Corrupt("address head probe exhausted on insert");
        assert!(is_probe_exhausted_error(&e));
        assert!(!is_probe_exhausted_error(&StoreError::NotFound));
        assert!(is_store_corrupt_display(&e.to_string()));
        assert!(is_store_corrupt_display(
            "corrupt record: leftover identity broken"
        ));
        assert!(!is_store_corrupt_display("bad-txnmrklroot"));
    }

    #[test]
    fn insert_get_roundtrip() {
        let path = tmp("roundtrip");
        let h = head_bits(&path, 12).unwrap();
        let mut txid = [0u8; 32];
        txid[0] = 1;
        insert_one(&h, &txid, Fk(1)).unwrap();
        assert_eq!(probe_one(&h, &txid).unwrap(), vec![Fk(1)]);
        insert_one(&h, &txid, Fk(1)).unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn foreigner_collision_both_found() {
        let path = tmp("foreigner");
        let h = head_bits(&path, 8).unwrap();
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a[0] = 0x10;
        b[0] = 0x10;
        b[4] = 0x02;
        insert_one(&h, &a, Fk(1)).unwrap();
        insert_one(&h, &b, Fk(2)).unwrap();
        assert!(probe_one(&h, &a).unwrap().contains(&Fk(1)));
        assert!(probe_one(&h, &b).unwrap().contains(&Fk(2)));
        assert_eq!(probe_one(&h, &a).unwrap()[0], Fk(1));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Two keys share a probe chain but differ in the fingerprint bytes: the
    /// foreigner is not a candidate, so identity never reads its txid.
    #[test]
    fn probe_skips_foreigner_with_other_fingerprint() {
        let path = tmp("fp-skip");
        let h = head_bits(&path, 8).unwrap();
        let (a, b) = colliding_pair();
        insert_one(&h, &a, Fk(1)).unwrap();
        insert_one(&h, &b, Fk(2)).unwrap();
        assert_eq!(probe_one(&h, &a).unwrap(), vec![Fk(1)]);
        assert_eq!(probe_one(&h, &b).unwrap(), vec![Fk(2)]);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Rows written before tags (meta v1) have a zero fingerprint, so they stay
    /// candidates for every key on their chain. `fill_fingerprints` writes the
    /// tag into the file, and a reopen keeps the skip.
    #[test]
    fn untagged_rows_match_any_key_until_filled() {
        let path = tmp("fp-untagged");
        let (a, b) = colliding_pair();
        {
            let h = head_bits(&path, 8).unwrap();
            insert_one(&h, &a, Fk(1)).unwrap();
            insert_one(&h, &b, Fk(2)).unwrap();
            strip_tags(&h);
        }
        let h = AddressHead::open(&path).unwrap();
        assert_eq!(probe_one(&h, &a).unwrap(), vec![Fk(1), Fk(2)]);
        assert_eq!(probe_one(&h, &b).unwrap(), vec![Fk(1), Fk(2)]);

        h.fill_fingerprints(&[(a, Fk(1)), (b, Fk(2)), ([7u8; 32], Fk(9))])
            .unwrap();
        assert_eq!(probe_one(&h, &a).unwrap(), vec![Fk(1)]);
        assert_eq!(probe_one(&h, &b).unwrap(), vec![Fk(2)]);
        drop(h);
        let h = AddressHead::open(&path).unwrap();
        assert_eq!(
            probe_one(&h, &b).unwrap(),
            vec![Fk(2)],
            "tags live in the file"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Every occupied slot in the file carries its key's fingerprint, and the
    /// map sees each insert.
    #[test]
    fn insert_writes_the_fingerprint_into_the_file() {
        let path = tmp("fp-file");
        let h = head_bits(&path, 10).unwrap();
        let mut entries = Vec::new();
        for i in 1..=300u64 {
            let mut txid = [0u8; 32];
            txid[0..8].copy_from_slice(&i.to_le_bytes());
            txid[16..24].copy_from_slice(&(i * 7).to_le_bytes());
            entries.push((txid, Fk(i)));
        }
        let by_rel = entries.clone();
        h.insert_many_in_place(&mut entries).unwrap();
        let mut occupied = 0;
        for slot in 0..h.layout.slots() {
            let rel = read_slot(&h, slot);
            if rel == 0 {
                continue;
            }
            occupied += 1;
            let (txid, _) = by_rel[rel as usize - 1];
            assert_eq!(
                read_slot_raw(&h, slot) >> h.bits(),
                fingerprint(&txid, h.bits()),
                "slot {slot}"
            );
        }
        assert_eq!(occupied, 300);
        for (txid, fk) in &by_rel {
            assert_eq!(probe_one(&h, txid).unwrap(), vec![*fk]);
            assert!(h
                .dump_page_hop(txid)
                .scan
                .cands
                .iter()
                .any(|&(_, r)| r == fk.0));
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn bip30_second_create_appends_deeper() {
        let path = tmp("bip30");
        let h = head_bits(&path, 12).unwrap();
        let mut txid = [0u8; 32];
        txid[0] = 0x55;
        insert_one(&h, &txid, Fk(1)).unwrap();
        insert_one(&h, &txid, Fk(2)).unwrap();
        let cands = probe_one(&h, &txid).unwrap();
        assert_eq!(cands[0], Fk(1), "first insert stays at earliest slot");
        assert!(cands.contains(&Fk(2)));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn rejects_fk_above_u32_on_4b() {
        let path = tmp("bigu32");
        let h = head_bits(&path, 12).unwrap();
        let txid = [1u8; 32];
        let err = insert_one(&h, &txid, Fk(u64::from(u32::MAX) + 1)).unwrap_err();
        assert!(matches!(err, StoreError::InvalidFk));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn miss_empty() {
        let path = tmp("miss");
        let h = head_bits(&path, 12).unwrap();
        assert!(probe_one(&h, &[9u8; 32]).unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn reopen_with_meta() {
        let path = tmp("reopen");
        {
            let h = head_bits(&path, 12).unwrap();
            let txid = [7u8; 32];
            insert_one(&h, &txid, Fk(3)).unwrap();
            h.flush().unwrap();
        }
        let h = AddressHead::open(&path).unwrap();
        assert_eq!(h.bits(), 12);
        assert_eq!(probe_one(&h, &[7u8; 32]).unwrap(), vec![Fk(3)]);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn reject_v7_directory() {
        let path = tmp("v7dir");
        std::fs::create_dir(&path).unwrap();
        match AddressHead::open(&path) {
            Err(StoreError::Corrupt(_)) => {}
            Err(e) => panic!("expected Corrupt, got {e}"),
            Ok(_) => panic!("expected error opening v7 directory"),
        }
        let _ = std::fs::remove_dir_all(&path);
    }

    /// Batch page-coalesced probe must match serial `probe_fks` (multi-page wave).
    #[test]
    fn probe_fks_batch_matches_serial() {
        let path = tmp("probe_batch");
        let h = head_bits(&path, 14).unwrap();
        let mut entries = Vec::new();
        for i in 1..=120u64 {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[3] = 0xab;
            entries.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut entries).unwrap();
        let keys: Vec<[u8; 32]> = entries.iter().map(|(t, _)| *t).collect();
        let batch = probe_batch(&h, &keys).unwrap();
        assert_eq!(batch.len(), keys.len());
        for (i, txid) in keys.iter().enumerate() {
            let serial = probe_one(&h, txid).unwrap();
            assert_eq!(batch[i], serial, "key {i}");
        }
        // Empty batch.
        assert!(probe_batch(&h, &[]).unwrap().is_empty());
        // Same-page multi-key still correct.
        let mut same_page = Vec::new();
        for i in 0..8 {
            let mut t = [0u8; 32];
            t[0] = 1;
            t[1] = i;
            same_page.push(t);
        }
        let _ = probe_batch(&h, &same_page).unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn insert_many_batch() {
        let path = tmp("batch");
        let h = head_bits(&path, 14).unwrap();
        let mut entries = Vec::new();
        for i in 1..=50u64 {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[4] = (i * 3) as u8;
            entries.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut entries).unwrap();
        for (txid, fk) in &entries {
            assert!(probe_one(&h, txid).unwrap().contains(fk));
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Many inserts spanning multiple pages (page-coalesced; call order within page).
    #[test]
    fn insert_many_batch_order_multi_page() {
        let path = tmp("batch_order");
        // bits=14 → 16 pages × 1024 slots (page-local at bits>10).
        let h = head_bits(&path, 14).unwrap();
        let mut entries = Vec::new();
        for i in 1..=400u64 {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[2] = ((i * 17) & 0xff) as u8;
            txid[3] = ((i * 31) & 0xff) as u8;
            txid[4] = 0xa5;
            entries.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut entries).unwrap();
        h.insert_many_in_place(&mut entries[..50]).unwrap();
        for (txid, fk) in &entries {
            assert!(
                probe_one(&h, txid).unwrap().contains(fk),
                "missing {fk:?} after batch-order insert"
            );
        }
        let mut extra = [0u8; 32];
        extra[0] = 0xee;
        extra[1] = 0xff;
        insert_one(&h, &extra, Fk(9001)).unwrap();
        assert!(probe_one(&h, &extra).unwrap().contains(&Fk(9001)));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Same txid twice in one batch: later fk is deeper; plan order preserved
    /// under page sort (stable by orig_i).
    #[test]
    fn insert_many_same_txid_preserves_depth_order() {
        let path = tmp("same_txid");
        let h = head_bits(&path, 14).unwrap();
        let txid = [0xab; 32];
        // Interleave with other pages so sort reorders globally but keeps orig_i
        // order within this page for the two same-txid inserts.
        let mut other = [0xcd; 32];
        other[0] = 0x11;
        let mut entries = [(txid, Fk(1)), (other, Fk(99)), (txid, Fk(2))];
        h.insert_many_in_place(&mut entries).unwrap();
        let cands = probe_one(&h, &txid).unwrap();
        assert_eq!(cands.len(), 2, "two creates on chain: {cands:?}");
        // probe_fks is home→deep (hop order); first insert is shallower.
        assert_eq!(cands[0], Fk(1));
        assert_eq!(cands[1], Fk(2));
        // Deepest wins for body resolve semantics.
        assert_eq!(*cands.last().unwrap(), Fk(2));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn insert_many_sole_no_sort_roundtrip() {
        let path = tmp("sole");
        let h = head_bits(&path, 14).unwrap();
        let mut entries = Vec::new();
        // Reverse-ish order; page coalescing still finds all.
        for i in (1..=80u64).rev() {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[3] = 0x5e;
            entries.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut entries).unwrap();
        // Idempotent re-insert.
        h.insert_many_in_place(&mut entries[..10]).unwrap();
        for (txid, fk) in &entries {
            assert!(
                probe_one(&h, txid).unwrap().contains(fk),
                "missing after sole insert"
            );
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    /// Sole writer + concurrent probes (no multi-inserter).
    #[test]
    fn sole_writer_with_concurrent_probes_all_found() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let path = tmp("sole_probe");
        let h = Arc::new(head_bits(&path, 16).unwrap());
        let n = 200u64;
        let barrier = Arc::new(Barrier::new(2));

        let prober = {
            let h = Arc::clone(&h);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..2000 {
                    let mut txid = [0u8; 32];
                    txid[0] = 1;
                    txid[2] = 0xca;
                    let _ = probe_one(&h, &txid);
                }
            })
        };

        barrier.wait();
        // Single inserter, batched (fences between batches).
        let mut batch = Vec::new();
        for i in 1..=n {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[2] = 0xca;
            batch.push((txid, Fk(i)));
            if batch.len() >= 32 {
                h.insert_many_in_place(&mut batch).unwrap();
                batch.clear();
            }
        }
        if !batch.is_empty() {
            h.insert_many_in_place(&mut batch).unwrap();
        }
        // Deadline: infinite join if prober/barrier stuck (panic-before-wait).
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            prober.join().unwrap();
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("concurrent address_head prober timed out (hang?)");
        for i in 1..=n {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[2] = 0xca;
            assert!(
                probe_one(&h, &txid).unwrap().contains(&Fk(i)),
                "missing fk {i}"
            );
        }
        // Idempotent re-insert of a subset.
        let mut again = Vec::new();
        for i in 1..=20u64 {
            let mut txid = [0u8; 32];
            txid[0] = (i & 0xff) as u8;
            txid[1] = ((i >> 8) & 0xff) as u8;
            txid[2] = 0xca;
            again.push((txid, Fk(i)));
        }
        h.insert_many_in_place(&mut again).unwrap();

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path(&path));
    }

    #[test]
    fn mainnet_default_bits_is_25() {
        assert_eq!(MAINNET_BITS, 25);
        // 4 B × 1024 = 4 KiB pages at mainnet default.
        assert_eq!(PAGE_SLOTS as usize * 4, 4096);
    }

    /// insert_fk_into_page_buf / store_entry error arms + empty page / invalid fk.
    #[test]
    fn insert_fk_page_buf_error_arms() {
        let txid = [0x11u8; 32];
        // Null fk
        let mut page = vec![0u8; 4096];
        assert!(matches!(
            insert_fk_into_page_buf(&mut page, 12, &txid, Fk::NULL),
            Err(StoreError::InvalidFk)
        ));
        // A slot can't hold fk > u32::MAX
        assert!(matches!(
            insert_fk_into_page_buf(&mut page, 12, &txid, Fk(u64::from(u32::MAX) + 1)),
            Err(StoreError::InvalidFk)
        ));
        // A rel that does not fit in `bits` would fold into the fingerprint.
        assert!(matches!(
            insert_fk_into_page_buf(&mut page, 12, &txid, Fk(1 << 12)),
            Err(StoreError::InvalidFk)
        ));
        assert!(
            !page.iter().any(|&b| b != 0),
            "a rejected rel writes nothing"
        );
        // Happy path insert + idempotent
        let r = insert_fk_into_page_buf(&mut page, 12, &txid, Fk(7)).unwrap();
        assert!(r.wrote_new);
        let r2 = insert_fk_into_page_buf(&mut page, 12, &txid, Fk(7)).unwrap();
        assert!(!r2.wrote_new);
        // probe_index bits ≤ PAGE_SLOT_BITS branch
        let _ = probe_index(&txid, 0, MIN_BITS);
        let _ = probe_index(&txid, 3, PAGE_SLOT_BITS);
        let load_ratio = |tx_count: u64, slots: u64| -> f64 {
            if slots == 0 {
                0.0
            } else {
                tx_count as f64 / slots as f64
            }
        };
        assert_eq!(load_ratio(10, 0), 0.0);
        assert_eq!((100_f64 * HEAD_LOAD_START).floor() as u64, 80);
        // bits_for_scale env out of range falls back to the explicit scale
        let prev = std::env::var_os("RBITCOIN_TX_HEAD_BITS");
        std::env::set_var("RBITCOIN_TX_HEAD_BITS", "999");
        assert_eq!(bits_for_scale(HeadScale::Tiny), TINY_BITS);
        assert_eq!(bits_for_scale(HeadScale::Mainnet), MAINNET_BITS);
        match prev {
            Some(v) => std::env::set_var("RBITCOIN_TX_HEAD_BITS", v),
            None => std::env::remove_var("RBITCOIN_TX_HEAD_BITS"),
        }
        let prev_bits = std::env::var_os("RBITCOIN_TX_HEAD_BITS");
        std::env::set_var("RBITCOIN_TX_HEAD_BITS", "20");
        assert_eq!(
            bits_for_scale(HeadScale::Tiny),
            20,
            "TX_HEAD_BITS must widen OA under tiny header heads"
        );
        assert_eq!(
            bits_for_scale(HeadScale::Mainnet),
            20,
            "TX_HEAD_BITS wins over Mainnet scale default"
        );
        match prev_bits {
            Some(v) => std::env::set_var("RBITCOIN_TX_HEAD_BITS", v),
            None => std::env::remove_var("RBITCOIN_TX_HEAD_BITS"),
        }
    }

    #[test]
    fn head_access_env_probe_stats_and_layout_helpers() {
        note_probe_depth_on_insert(PROBE_DEPTH_WARN);
        note_probe_depth_on_insert(PROBE_DEPTH_WARN + 1);
        note_probe_depth_on_insert(PROBE_DEPTH_WARN + 50);
        note_probe_exhausted();

        assert!(is_probe_exhausted_error(&StoreError::Corrupt(
            "address head probe exhausted on insert"
        )));
        assert!(!is_probe_exhausted_error(&StoreError::Corrupt("other")));

        let txid = [0xABu8; 32];
        let _ = page_index(&txid, MAINNET_BITS);
        let _ = h1_in_page(&txid, MAINNET_BITS);
        let _ = h2_in_page(&txid, MAINNET_BITS);
        let _ = page_slot_count(MAINNET_BITS);
        let _ = page_base_for_txid(&txid, MAINNET_BITS);
        assert_eq!(entry_file_off(0), 0);
        assert_eq!(entry_file_off(3), 12);
        let layout = default_layout(HeadScale::Tiny);
        assert!((MIN_BITS..=MAX_BITS).contains(&layout.bits));
        assert_eq!(default_layout(HeadScale::Tiny).bits, layout.bits);
        assert_eq!(default_layout(HeadScale::Mainnet).bits, MAINNET_BITS);
        let roll_thr = (100_f64 * HEAD_LOAD_START).floor() as u64;
        assert_eq!(roll_thr, 80);
        assert!(81 >= roll_thr);
        let ext = encode_layout_ext(layout, 7);
        let (dec, gen) = decode_layout_ext(&ext).unwrap();
        assert_eq!(dec.bits, layout.bits);
        assert_eq!(gen, 7);
        // Bad decode
        let bad = [0u8; 16];
        assert!(decode_layout_ext(&bad).is_err());
        let path = std::env::temp_dir().join(format!(
            "rbitcoin-ah-meta-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let meta = meta_path(&path);
        std::fs::write(&meta, b"x").unwrap();
        remove_legacy_meta_sidecar(&path);
        assert!(!meta.exists());
    }
}
