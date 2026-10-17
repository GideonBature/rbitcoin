Changed

- **macOS reads with libc pread by default.**
  The worker-pool session handed every read to a pool thread through one
  shared lock. In signet IBD that cost more than the reads: about 20% slower
  end to end with a warm or cold cache, and four times the system CPU.
  `RBITCOIN_IO=pool` still opens the pool session. Linux and Windows are
  unchanged.
