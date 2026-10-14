Changed

- Overnight missed mutants in confirm, logging, and the mempool are pinned or removed where the operator could not change a block, a peer message, or a reopened pool.
- Confirm progress, parent checks, and reject rewinds are pinned on the paths the confirm thread calls. Mempool compaction uses the dead-slot quarter, worst-chunk order keeps the earlier tied rate, eviction stops when a slot is free, and a package that spends a later txid is not topological.
