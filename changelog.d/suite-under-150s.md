Changed

- **P2P shutdown wakes dial, accept, and connect-retry.** Those tasks were blocked on a channel or a tick, so every shutdown sat out the full 750 ms grace and then aborted them. The grace remains for a session that is still unwinding.
- **`--max-run-secs` exits when the deadline is reached.** The tip loop used to notice it only on the 5 s perf tick.
- **IBD densify assign-stop is a field on the IBD config.** `RBITCOIN_BLOCK_QUEUE_BYTES` / `_GB` are still read once when a production config is built. Tests pass the cap in, so they no longer serialize on a process-wide env lock.
- **Test dependencies compile at opt-level 2.** Workspace crates stay unoptimized. Dev and test builds still keep line tables.
- **The hub `OP_TRUE` signet generate test uses regtest difficulty.** Accept still requires the BIP325 solution. The real signet target stays covered by the consensus miner test.
- **`NODE_COMPACT_FILTERS` lives on the peer hub.** Version, feeler, addr, and `getnetworkinfo` read that node's bit. There is no process-wide flag.
- **`getblocktemplate` stays off while the catch-up pool still holds transactions.** That returns `-10`. An empty pool is served, and a node with no mempool still serves `getblocktemplate` on the chain tip and an empty `getrawmempool`. `-blocksonly` returns a permanent error for `getblocktemplate` so a miner does not retry. `generate` during catch-up mines the coinbase only. Once relay is on, `getblocktemplate` and Stratum v2 build on the tip recorded after the pool strip. A Stratum v2 session that is still holding tries again on the template interval, so the tip that ends catch-up is not missed when relay turns on after that event. A block connect and an initial-download confirm batch hold that mark from before the chain tip moves until the drop is stored. A disconnect, including a header rewind, moves it to the fork point the pool was put back on. The template mark is a hash already in hand. Enabling relay looks up remaining prevouts in the store while the mempool write lock is held.
- **`cargo llvm-cov` no longer builds the workspace twice.** The coverage script was compiling plain node and cli bins before the instrumented test build. The suite already builds the bins it execs.

Fixed

- **Default journeys link and run as one test binary.** Doctests are off on library crates that have none. The node, cli, and journey-node bins do not build an empty test harness.
