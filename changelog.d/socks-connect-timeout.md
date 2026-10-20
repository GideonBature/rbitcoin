Changed

- **SOCKS dials get 30 s to connect.** Each isolated SOCKS dial builds its
  own Tor circuit, and a 32-dial IBD wave queues in Tor for longer than the
  8 s clearnet bound, so whole waves timed out. Clearnet dials through
  `--proxy` and onion dials now get at least 30 s. Direct and CJDNS dials
  keep 8 s and I2P keeps 90 s. The rule lives on `Dialer::connect_timeout`,
  which replaces `connect_timeout_for`; the floor is a `Dialer::Socks` field.
