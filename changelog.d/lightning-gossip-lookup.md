Changed

- **Lightning gossip lookup is an application connector.** `docs/lightning.md`
  separates channel sync from `UtxoLookup`. The position route is served
  without `--sh-index`. Stock ldk-node, CLN, and LND do not call it.
