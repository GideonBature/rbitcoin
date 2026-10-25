Fixed

- **Onion seed nodes use the configured SOCKS proxy.** `--seed-node` and `seed_node=` onion addresses no longer fail at runtime with `bad seednode address: invalid IP address syntax`. Both the empty-address-book dial and delayed fallback use overlay-aware dialing. Seed nodes remain disabled under `connect=`.
