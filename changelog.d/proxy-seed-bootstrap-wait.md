Fixed

- **A fresh `--proxy` datadir waits for its seeds.** Catch-up read the dial
  targets before any SOCKS seed addr-fetch had answered, so a node with an
  empty book shut down about 60 ms after start. It now waits until a seed
  has answered and its session has ended, and re-queues the seeds every
  60 s until then.
