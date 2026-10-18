Fixed

- **A txid that repeats inside one confirm batch is handled like Core.**
  Below BIP34 a coinbase can repeat once its first copy is spent. A spend now
  uses the copy that exists at its height, the outputs of a recreated
  transaction can be spent again, and BIP30 counts spends made earlier in the
  same batch. Before, IBD could halt on such a chain that Core accepts. Only
  reachable below BIP34 (for example regtest with
  `-testactivationheight=bip34@N`).
