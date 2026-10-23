Fixed

- **Sparrow batches address lookups.** Sparrow sends JSON-RPC batches
  only to servers whose `server.version[0]` reads as `electrs/` 0.9.0 or
  later, so it sent one request per address against `rbitcoin-electrs <ver>`.
  `server.version[0]` is now `electrs/0.10.0-rbitcoin-<ver>`. The Electrum
  listener now answers a JSON array line as a JSON-RPC 2.0 batch: one array
  line back, same order, a per-item error that does not spoil the rest,
  written one slot at a time. Notifications get no slot.
  Before, a batch got one `unknown method` error with `id: null`. An empty
  batch, or one over 345 items, is one `-32600` error. The two streaming
  subscribes (`blockchain.tweaks.subscribe`,
  `blockchain.silentpayments.subscribe`) get a per-item error inside a batch
  (#1152).
