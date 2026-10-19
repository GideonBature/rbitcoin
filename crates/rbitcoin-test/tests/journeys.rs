//! Default journeys in one harness.
//!
//! Separate `tests/*.rs` targets each linked the whole workspace and ran
//! one after another. Modules here share one process, so `OnceLock` pads
//! are built once and the harness runs journeys on four threads.
//! `overlay` stays a feature-gated target of its own.

#[path = "asmap_journey.rs"]
mod asmap_journey;
#[path = "consensus_rules.rs"]
mod consensus_rules;
#[path = "core_analogs.rs"]
mod core_analogs;
#[path = "cross_surface.rs"]
mod cross_surface;
#[path = "electrum_protocol.rs"]
mod electrum_protocol;
#[path = "end_of_ibd.rs"]
mod end_of_ibd;
#[path = "integration_multinode.rs"]
mod integration_multinode;
#[path = "scenarios.rs"]
mod scenarios;
