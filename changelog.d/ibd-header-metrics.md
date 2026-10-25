### Fixed

- Health metrics and `getblockchaininfo` now include IBD's locally accepted header walk, including look-ahead headers and restored checkpoints. Header height no longer falls back to the validated block tip during catch-up, incorrectly reporting verification progress as complete. Peer-advertised horizons do not count as accepted headers. Both IBD and stored header tips use one reporting cache; invalidation and reconsideration recheck candidate ancestry off the scrape path, without permanently silencing unrelated valid walks.
