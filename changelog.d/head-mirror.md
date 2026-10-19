Changed

- **IBD looks up txids in RAM.** The open `tx.head` segment keeps its slot
  array in memory (128 MiB per unsealed segment) with a 7-bit fingerprint of
  each key. A probe no longer reads a 4 KiB page from the file, and most
  foreign slots on a probe chain are skipped before the identity read of
  `txid.body`. Signet IBD on the Mac mini ran 18% faster. The file format is
  unchanged. A restart refills the fingerprints from `txid.body`, one mix per
  row, in parallel. The open segment's uring page-probe stream is gone;
  sealed segments are untouched.
- **`tx.head` slots are 4 bytes only.** The 8-byte entry width (bits 33 and
  34) is removed; `RBITCOIN_TX_HEAD_BITS` tops out at 32.
