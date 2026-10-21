Fixed

- **IBD drops a competing fork's block that does not connect.** A batch of
  blocks whose parent is not this chain was requeued as a stale plan, and
  IBD could sit short of a valid tip with that height still occupied. The
  height is freed and that block is not fetched again. A block whose parent
  is the tip, or this chain's block still being written, is still retried.
- **`generate` removes the pool transactions it mines before it returns.**
  The set placed in the block was a separate step from the set deleted, and
  a strip while relay is off deletes nothing, so those transactions could
  still be listed afterward. They leave the pool in the same step that
  chooses them. A node still catching up mines only the coinbase.
