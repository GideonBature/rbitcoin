Changed

- **Mainnet headers are no longer rejected by historical checkpoint hashes.** Bitcoin Core v31.1 does not enforce `mapCheckpoints`. A header at one of the old heights (through 295000) is accepted when its parent, median time, version, `nBits`, and proof of work are valid. Initial-download lookahead checkpoints in `header.adopt` are unchanged.
