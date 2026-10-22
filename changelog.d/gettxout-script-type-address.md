Fixed

- **`gettxout` reports the script `type` and `address`.** Its
  `scriptPubKey` carried only `hex` and `asm`. It is now the same object
  `getrawtransaction` and `getblock` return, with Core's `type` and, for a
  standard script, `address`, for a mempool or a confirmed coin.
