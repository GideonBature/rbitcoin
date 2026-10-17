Fixed

- **A confirmed-set seed does not put a disconnected block back.** The background fill publishes each height only when that hash is still the header there, so a reorg during the walk cannot make `has_block` true for a block that is no longer on the chain.
