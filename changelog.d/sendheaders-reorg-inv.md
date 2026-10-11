Fixed

- **After a reorg longer than eight blocks, the next block stays an inv
  announcement.** A header mark on the block before the inv-only tip no
  longer resumes headers until the peer asks.
