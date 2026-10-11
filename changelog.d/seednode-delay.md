Fixed

- **A seednode is added after 10s of failed addrman dials even when
  `setmocktime` runs during catch-up.** The wait starts when outbound
  dials start. A mock-time jump during catch-up still crosses that mark.
