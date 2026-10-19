Changed

- **P2P shutdown wakes dial, accept, and connect-retry.** Those tasks were blocked on a channel or a tick, so every shutdown sat out the full 750 ms grace and then aborted them. The grace remains for a session that is still unwinding.
- **`--max-run-secs` exits when the deadline is reached.** The tip loop used to notice it only on the 5 s perf tick.
- **IBD densify assign-stop is a field on the IBD config.** `RBITCOIN_BLOCK_QUEUE_BYTES` / `_GB` are still read once when a production config is built. Tests pass the cap in, so they no longer serialize on a process-wide env lock.
- **Test dependencies compile at opt-level 2.** Workspace crates stay unoptimized. Dev and test builds still keep line tables.
- **The hub `OP_TRUE` signet generate test uses regtest difficulty.** Accept still requires the BIP325 solution. The real signet target stays covered by the consensus miner test.
- **`NODE_COMPACT_FILTERS` lives on the peer hub.** Version, feeler, addr, and `getnetworkinfo` read that node's bit. There is no process-wide flag.
- **`getblocktemplate`, `getrawmempool`, and `generate` stay off while the pool is not maintained.** Relay off (still catching up) returns `-10`. A node with no mempool still serves `getblocktemplate` on the chain tip and an empty `getrawmempool`. `-blocksonly` returns a permanent error so a miner does not retry. `generate` during catch-up mines the coinbase only. Once relay is on, `getblocktemplate` and Stratum v2 build on the tip recorded after the pool strip. A Stratum v2 session that is still holding tries again on the template interval, so the tip that ends catch-up is not missed when relay turns on after that event. A block connect and an initial-download confirm batch hold that mark from before the chain tip moves until the drop is stored. A disconnect moves it to the fork point the pool was put back on. There is no chain scan under the mempool lock.
- **A node built with the journey `test-hooks` feature warns at startup.** That build uses the short reconnect and SV2 cooldown. Release, nix, and `cargo build -p rbitcoin-node` do not.
- **`cargo llvm-cov` no longer builds the workspace twice.** The coverage script was compiling plain node and cli bins before the instrumented test build. The suite already builds the bins it execs.

Fixed

- **Default journeys link and run as one test binary.** Doctests are off on library crates that have none. The node, cli, and journey-node bins do not build an empty test harness. Journeys compile a 50ms SAM retry, a 250ms connect redial, and a 100ms SV2 budget cooldown. The node binary keeps 500ms, 2s, and 1s.
