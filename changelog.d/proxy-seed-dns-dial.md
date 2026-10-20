Fixed

- **`--proxy` no longer panics on the DNS seed addr-fetch dials.**
  With a proxy, each DNS seed is dialed by name through SOCKS. The
  outbound session parsed that name as an overlay address and panicked
  the dial task. The seed is now dialed by name, with the `0.0.0.0:port`
  peer identity it had before the overlay `NetAddr` tracking.
