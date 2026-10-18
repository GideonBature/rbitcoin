Fixed

- **`submitblock` accepts a side-branch block that spends its side parent's
  outputs.** It used to answer `bad-txns-inputs-missingorspent` and remember
  the block as invalid, so the node then refused that chain from peers too
  and stayed on the old tip. For a block whose parent is off the best chain,
  input checks now wait for the branch connect, as in Core. A heavier branch
  that fails there returns Core's reason (`bad-txns-in-belowout`,
  `bad-txns-inputs-missingorspent`, `bad-txns-premature-spend-of-coinbase`)
  instead of `consensus: in < out` and similar.
