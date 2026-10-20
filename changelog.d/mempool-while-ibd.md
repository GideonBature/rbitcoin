Fixed

- **`getrawmempool` and `getblocktemplate` answer on an empty catch-up pool, and `-blocksonly` still serves the raw mempool and `generate`.** Core's functional harness syncs mempools and builds a regtest template before the tip is recent, and it mines on a `-blocksonly` node. A catch-up pool that still holds transactions returns `-10` for both. `-blocksonly` still refuses `getblocktemplate`.
