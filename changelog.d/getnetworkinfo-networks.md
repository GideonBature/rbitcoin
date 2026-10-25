Fixed

- **`getnetworkinfo` lists its networks.** `networks` was always empty.
  It now has a row for ipv4, ipv6, onion, i2p, and cjdns, with whether
  the node can reach it and which proxy it uses, like Core.
