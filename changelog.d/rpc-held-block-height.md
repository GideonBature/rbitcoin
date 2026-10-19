Fixed

- **`getblockheader` and `getblock` on a held side block report its height
  and chain work, as Core does.** Both left out `height`. `getblockheader`
  reported only that block's own work as `chainwork`, and so did
  `getblock` when the parent was not on the active chain. Both still need
  the block's ancestors to be held or stored. Once a held ancestor is
  evicted from the held-body cache, `height` is left out and `chainwork`
  is that block's own work, as before.
