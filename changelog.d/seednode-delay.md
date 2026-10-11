Fixed

- **A seednode is added 10s after the first outbound dial, including when
  `setmocktime` jumps the clock in that window.** RPC is listening before
  catch-up, so the jump can land before those 10s elapse.
