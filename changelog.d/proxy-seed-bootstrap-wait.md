Fixed

- **A fresh `--proxy` datadir waits for its seeds.** Catch-up read the dial
  targets before any SOCKS seed addr-fetch had answered, so a node with an
  empty book shut down about 60 ms after start. It now waits up to 180 s
  for a seed's `getaddr` reply to be learned (a self-announce does not
  count) and re-queues the seeds every 60 s while no addr-fetch session is
  live. With no reply, the empty book takes the incomplete path as before.
