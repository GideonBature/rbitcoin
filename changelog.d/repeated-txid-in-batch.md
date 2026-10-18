Fixed

- **A txid that repeats inside one confirm batch is handled like Core.**
  A coinbase can repeat once its first copy is spent: below BIP34, or after
  BIP34 when an old coinbase already pushed the later height. A spend now uses
  the copy that exists at its height, the outputs of a recreated transaction
  can be spent again, and BIP30 counts spends made earlier in the same batch.
  Before, IBD could halt on such a chain that Core accepts.
