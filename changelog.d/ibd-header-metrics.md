### Fixed

- Health metrics and `getblockchaininfo` now include IBD's locally accepted header walk, including look-ahead headers and restored checkpoints. Header height no longer falls back to the validated block tip during catch-up, incorrectly reporting verification progress as complete. Peer-advertised horizons do not count as accepted headers. Invalidating the chain clears the IBD report and rejects stale publications without walking header ancestry on every scrape.
