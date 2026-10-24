Added

- **`scriptPubKey` carries Core's `desc`.** `getrawtransaction`,
  `decoderawtransaction`, `getblock` verbosity 2, `decodescript`,
  `gettxout`, and REST `getutxos` now return the inferred output
  descriptor with its checksum: `pk`, `multi`, `rawtr`, `addr`, or `raw`,
  as Core infers it with no keystore.
