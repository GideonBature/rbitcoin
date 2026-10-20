Fixed

- **Release builds compile again.** The crate-layout move gated a `rbitcoin-store` import and two `rbitcoin-query` re-exports behind `cfg(debug_assertions)` while production code used them, so `cargo build --release`, `release.yml`, and the nix package failed. The suites themselves build in the dev and test profiles only.
- **A debug node no longer grows the confirm spindle trace.** `spindle_mark` records only after a test arms it on that thread. Before, every debug build appended to a thread-local on each confirm batch for the life of the process.
- **The coverage badge gate is the 91% floor.** The badge still said 89 after the floor moved, so a run between 89 and 91 would have drawn a passing badge on a failing gate. `coverage-gate.py` is now the only place the number lives; the badge reads it from `gate.json`.
- **`cargo doc --workspace` builds.** The layout move made private modules `#[doc(hidden)] pub`, so rustdoc began resolving 37 links in them that had never been checked.

Changed

- **`qc` compiles the release profile and the workspace rustdoc.** `cargo check --release` runs in the workspace shape and in the product shape nix and `release.yml` build; `cargo doc --workspace --no-deps` runs after it.
- **Suite roots are linted like `src/`.** The `#![allow(unused_imports, dead_code, unused_variables)]` on every `tests/suite.rs` is gone; the imports the move left behind and one dead env-lock helper went with it.
