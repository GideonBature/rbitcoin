Changed

- **Electrum batches answer read-only items four at a time.** A batch
  line ran its items one after another, so a 100-address `get_history`
  page cost the sum of 100 lookups. Read-only items (scripthash history,
  balance, unspent and mempool, transaction and header reads, fee
  estimates) now run up to four at once per connection and still land in
  request order. A subscribe, unsubscribe or other stateful item waits for
  the reads before it and runs alone. Slot `i` is written before read
  `i + 4` starts, so a client that stops reading still stops the work
  (#1174).
