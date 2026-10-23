//! Electrum protocol 1.4+ server for **wallet clients** (confirmed + optional
//! mempool / libre-relay-class).
//!
//! Clients already know their scripthashes / txids.
//!
//! `server.version[0]` is `electrs/0.10.0-rbitcoin-<workspace.package.version>`.
//! We are not electrs. Cake Wallet `getNodeIsElectrs()` requires the
//! substring `electrs` before it will probe `blockchain.tweaks.subscribe`.
//! Sparrow batches only for `electrs/` at 0.9.0 or later.
//! Other tweaks clients (kiss-bdk) use the same stream without that probe.

#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod server;
#[doc(hidden)]
pub mod silent_scan;
#[doc(hidden)]
pub mod tweaks;
#[doc(hidden)]
pub mod unspent;

pub use server::{
    electrum_scripthash_hex, parse_electrum_request_line, perf_totals, run_electrum,
    sample_reset_perf, ElectrumConfig, ElectrumHandle, ServeLimits, TipNotify,
    DEFAULT_MAX_SCRIPTHASH_SUBS,
};
pub use tweaks::DEFAULT_TWEAKS_MIN_DUST;
pub use unspent::{
    scripthash_mempool_stats_slot, scripthash_utxos_with_mempool_slot_in, MempoolShStats,
};
