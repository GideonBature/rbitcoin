Removed

- **Unreachable helpers.** Dropped confirm meters, store wrappers, and address-book constructors that no production path, test, or fuzz target called. Shipped confirm, IBD, and index behavior is unchanged.
- **Cold scripthash bulk load.** Dropped the resume session and the open-address pre-size hint. The index still packs through the shard pack session. `RBITCOIN_SH_UNIQUE_HINT` is ignored.
