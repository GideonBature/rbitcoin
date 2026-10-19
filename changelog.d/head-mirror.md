Changed

- **IBD looks up txids through a map of the open `tx.head`.** The open
  segment's slot file is read through a read-only `MAP_SHARED` view, so a
  probe no longer reads a 4 KiB page per key. Each slot also carries a 7-bit
  fingerprint of its txid in the spare bits of the 4-byte entry, so most
  foreign slots on a probe chain are skipped before the identity read of
  `txid.body`. Signet IBD on the Mac mini ran about 20% faster. The open
  segment's uring page-probe stream is gone; sealed segments are untouched.
- **`tx.head` meta is v2.** An existing head opens as before; its open
  segment gets fingerprints written from `txid.body` on that first open, once,
  then the meta records it as tagged. A downgrade to a binary that expects
  meta v1 rebuilds `tx.head` from Class A.
- **`tx.head` slots are 4 bytes only.** The 8-byte entry width (bits 33 and
  34) is removed; `RBITCOIN_TX_HEAD_BITS` tops out at 32.
