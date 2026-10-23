Added

- **`--trust-peer ADDR`** (conf `trustpeer=`) dials only that peer and still
  runs consensus checks. This is not a faster sync. The address grammar
  matches `--connect`. The node turns P2P listen off, sets `max-inbound` to
  0, and does not use DNS seeds, fixed seeds, `--seed-node`, or peers-file
  rows. A missing address, a second address, a different `--connect`,
  `--seed-node`, `--listen`, `--listen-onion`, or `--max-inbound` greater
  than 0 is a start error before the store opens. The same address given
  again as `--connect` is accepted. `addnode` and `addconnection` of any
  other address fail and name the trusted peer. A warn-and-apply fault from
  that peer's current session is logged `trust-peer: applied` when that
  transaction is kept. A script fault, and a lookup or load structural
  warn, are logged only after that block connects. A header fault is
  logged when that header check is kept.
  Proof-of-work, nBits, and a body that does not match its header are not
  applied. Local submit, block proposals, SV2 submission, and wallet
  broadcast stay strict and do not log a `trust-peer:` line. A signet
  block that does not parse is warn-and-apply from that session; encoding
  a signet commitment stays a hard error. With `--proxy` or `--onion`,
  this node's own transactions are still announced on that one session.
  Isolated broadcast stays off. A script or non-final transaction admitted from
  that session stays through reorg eviction and a replacement that does not
  commit. Opening with the mode off removes those transactions. If a marked body or a descendant of it cannot be removed, open fails and the mark stays. The mark
  is loaded again only while the mode stays on. A block
  template skips a marked transaction, and a locally submitted block
  rechecks that transaction's script. The trusted session is not a
  high-bandwidth compact-block peer. A hostname keeps one current socket.
  While the mode is on, `warnings` on `getnetworkinfo` and
  `getblockchaininfo` names that peer. That line does not run
  `-alertnotify`. The block template includes that line, and omits the
  `warnings` key when the array would be empty. The fault
  list is `COMPAT.md`. `services.rbitcoin.trustPeer` passes the flag with
  listen off and `max-inbound` 0.
