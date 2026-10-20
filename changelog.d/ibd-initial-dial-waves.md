Fixed

- **Catch-up dials the next wave when the first reaches no one.** One
  initial batch that failed ended catch-up with `no peers connected` while
  the book still held untried addresses. IBD now dials up to
  `initial_dial_waves` (8) waves of untried addresses before giving up.
  The stale-tip retry keeps a single wave.
