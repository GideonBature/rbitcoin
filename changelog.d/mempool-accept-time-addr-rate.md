Fixed

- **Mempool entries keep their accept time across a restart.** The mempool sidecar is schema 4: each record stores its accept time, so `getmempoolentry.time` is no longer 0 after a restart and `-mempoolexpiry` still removes old entries. A schema 1–3 sidecar converts on the next open, and its entries count their accept time from that open (#1029).
- **Peer addresses are rate limited before they reach the address book.** Each peer starts with one address token and earns one per 10 s (up to 1000), as in Core. A peer can no longer replace the address book with a few 1000-entry `addrv2` messages. The IBD path uses the same bucket. `getpeerinfo` reports `addr_processed` and `addr_rate_limited`. Peers with the `addr` whitelist permission are not limited (#1030).
