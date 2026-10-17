Changed

- **Consensus rules no longer open the store.** `rbitcoin-consensus` decides header, block, and transaction accepts and rejects from bytes and caller facts. `rbitcoin-query` loads rows, writes Class A, Class C, and spend annotations, and calls those functions. A disk, io_uring, or cancel fault stays a pipeline error and is not cached as an invalid block.
- **Electrum opens after the follow header round.** A block mined while the scripthash index materializes is connected and indexed before the wallet port opens.
