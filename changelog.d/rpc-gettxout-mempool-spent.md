Fixed

- **`gettxout` hides an output of a mempool transaction that another mempool
  transaction spends.** The confirmed branch already did. A wallet that
  picked coins with `gettxout` (default `include_mempool`) saw its own
  unconfirmed change as still spendable.
