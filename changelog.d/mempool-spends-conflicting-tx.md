Fixed

- **The mempool rejects a replacement that spends an output of a tx it
  replaces** (`bad-txns-spends-conflicting-tx`, as in Core). It used to be
  admitted with an input that left with the replaced tx, so
  `getblocktemplate` offered a block that fails
  `bad-txns-inputs-missingorspent`.
