# 098 — Overlay misbehavior is not remembered

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

Discouragement was keyed by the TCP address. An onion or I2P session's version socket is `0.0.0.0`, which is skipped so the refusal is never stored. The in-memory day-long set now keys those peers by their `NetAddr`. A later session with that net is discouraged. Loopback and `0.0.0.0` identities are still skipped. Nothing is written to `banlist.dat`.

An outbound dial checks that same key. A netgroup paused after an inbound slot eviction still cannot take another inbound slot. It does not refuse dials to the rest of the prefix.

**Regression:** `rbitcoin-net` `overlay_misbehavior_discourages_that_net`, `slot_evict_does_not_block_an_outbound_dial`.
