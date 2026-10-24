Fixed

- **A long addr-fetch is counted after the session is gone.**
  `p2p_timeout_getaddr_and_keepalive_ping` waited to see the live AddrFetch
  row, then see it leave. Under coverage that row can connect, learn the
  reply, and disconnect between snapshots, so the journey failed with
  `saw=false` while the uninstrumented run passed. It now waits until
  `addrfetch_replies` moves, which stays set after the disconnect.
