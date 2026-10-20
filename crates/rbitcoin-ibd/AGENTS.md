# rbitcoin-ibd

Multi-peer initial block download and the confirm pipeline. Depends on
net, query, store, consensus, log, and primitives. In the production
graph only `rbitcoin-node` and `rbitcoin-test` depend on this crate; RPC,
Electrum, Esplora, and SV2 do not (`rbitcoin-net` dev-depends on it for
`peer_header_sync_journey`). Tip follow and relay stay in `rbitcoin-net`.

## Read first

Open the row that matches the change. Leave the other owners closed.

| Change | Read |
|--------|------|
| Roles, body queue, publish order, `ibd: perf` fields | [`docs/concurrency.md`](../../docs/concurrency.md) |
| Confirm stage IO, leftover union, pin identity | [`docs/invariants.md`](../../docs/invariants.md) |
| Process RAM, body-queue caps, evict | [`docs/ibd-memory.md`](../../docs/ibd-memory.md) |
| IO machines | [`docs/io-modality.md`](../../docs/io-modality.md) |

## Where

- Assign and densify: `src/ibd/assign.rs`, `src/ibd/assign_plan.rs`, `src/ibd/body.rs`
- Peers and dial: `src/ibd/dial.rs`, `src/ibd/peer_io.rs`, `src/ibd/events/`
- Confirm: `src/ibd/confirm/`
- Exit, reorg, archive: `src/ibd/exit.rs`, `src/ibd/reorg.rs`, `src/ibd/archive.rs`
- Progress and perf lines: `src/ibd/progress.rs`, `src/ibd/perf_log.rs`

## Verify

`cargo test -p rbitcoin-ibd --test suite`

Core functional (labeled job only): [`.agents/skills/core-functional/SKILL.md`](../../.agents/skills/core-functional/SKILL.md).
