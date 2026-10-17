Fixed

- **Spending an unspendable output fails even when scripts are skipped.** An output whose script starts with `OP_RETURN`, or is longer than 10,000 bytes, is not a coin. The spend is a missing prevout on connect and `bad-txns-inputs-missingorspent` on a block proposal, with scripts on and under a height-only milestone. A 10,000-byte script that does not start with `OP_RETURN` is still a coin. The output stays in the archive.
