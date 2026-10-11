Fixed

- **A seednode is added 10s after RPC is up once an outbound dial has been
  tried, including when `setmocktime` jumps the clock in that window.** The
  dial itself is often during catch-up, before the port accepts.
