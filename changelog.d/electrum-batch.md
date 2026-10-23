Fixed

- **Sparrow batches address lookups.** Sparrow sends JSON-RPC batches
  only to servers whose `server.version[0]` it recognizes, so it sent one
  request per address against `rbitcoin-electrs <ver>`. `server.version[0]`
  is now `rbitcoin-electrs <ver> electrumx`: Sparrow batches on `electrumx`,
  BlueWallet keeps batching, and Cake still finds `electrs`. The Electrum
  listener now answers a JSON array line as a JSON-RPC 2.0 batch: one array
  line back, same order, a per-item error that does not spoil the rest,
  written one slot at a time. Notifications (no `id`) are skipped.
  Before, a batch got one `unknown method` error with `id: null`. An empty
  batch, or one over 345 items, is one `-32600` error. The two streaming
  subscribes (`blockchain.tweaks.subscribe`,
  `blockchain.silentpayments.subscribe`) get a per-item error inside a batch
  (#1152).
