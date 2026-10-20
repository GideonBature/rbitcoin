Fixed

- **Release builds compile again.** The crate-layout move gated a `rbitcoin-store` import and two `rbitcoin-query` re-exports behind `cfg(debug_assertions)` while production code used them, so `cargo build --release`, `release.yml`, and the nix package failed. `qc` now runs `cargo check --release --workspace --exclude rbitcoin-bench`; the suites themselves build in the dev and test profiles only.
- **A debug node no longer grows the confirm spindle trace.** `spindle_mark` records only after a test arms it on that thread. Before, every debug build appended to a thread-local on each confirm batch for the life of the process.
- **The coverage badge gate is the 91% floor.** The badge still said 89 after the floor moved, so a run between 89 and 91 would have drawn a passing badge on a failing gate. `coverage.test.sh` pins the badge to `FLOOR_PCT`.
- **Suite roots are linted like `src/`.** The `#![allow(unused_imports, dead_code, unused_variables)]` on every `tests/suite.rs` is gone; the imports the move left behind and one dead env-lock helper went with it.
