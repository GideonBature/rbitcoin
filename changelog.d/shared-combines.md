Changed

- **Shared helpers for tests and internal copies.** Regtest mining, store
  copy, P2WPKH verify, positional IO completion, scripthash slot paging,
  Electrum parameter reads, legacy filename renames, IBD stage counters,
  mempool snapshot refresh, I2P base64, script-test hex, tweak-test headers,
  the regtest ChainHub adapter, bench wallet fan-out, the JSON-RPC test
  client, differential fuzz setup, and contract-test asserts each have one
  owner. Electrum rejects a non-string parameter with `param {idx} expected
  string`. Core script-test hex uses the primitives decoder, including a
  `0x` prefix and the `odd hex length` error. Operator consensus, RPC, and
  IBD behavior otherwise stay the same.
