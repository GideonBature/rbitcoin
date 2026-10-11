Fixed

- **A seednode is added 10s after the first outbound dial, including when
  `setmocktime` jumps the clock in that window.** The wait does not start
  at process start, so a slow startup cannot emit the line before the dial
  is observed.
