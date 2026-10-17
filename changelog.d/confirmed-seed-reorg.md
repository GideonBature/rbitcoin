Fixed

- **A confirmed-set seed does not put a disconnected block back.** The background fill copies the height index already built at open, under the same lock a reorg holds while it drops blocks. It does not read the header store again, and a block removed before the copy stays out of `has_block`.
