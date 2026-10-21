Fixed

- **IBD drops a competing fork's block that does not connect.** A batch of
  blocks whose parent is not this chain was requeued as a stale plan, and
  IBD could sit short of a valid tip with that height still occupied. The
  height is freed and that block is not fetched again. A block whose parent
  is the tip, or this chain's block still being written, is still retried.
