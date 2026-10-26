Fixed

- **`gettxout` returns null for a confirmed unspendable output.** An
  `OP_RETURN` output (including the witness commitment) or a script over
  10,000 bytes was reported as an unspent coin. Core never adds these to
  the UTXO set, and rbitcoin already refuses to spend them. An unspent
  mempool output is still listed. One spent by another mempool tx is null.
