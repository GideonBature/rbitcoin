# 098 — Overlay misbehavior is not remembered

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

Discouragement was keyed by the TCP address. An onion or I2P session's version socket is `0.0.0.0`, which is skipped so the refusal is never stored. The in-memory day-long set now keys onion and I2P by the 32-byte identity, so another port is the same peer. Loopback and `0.0.0.0` identities are still skipped. Nothing is written to `banlist.dat`.

An outbound dial checks that same key, including a feeler and an IBD dial. A netgroup paused after an inbound slot eviction still cannot take another inbound slot. It does not refuse dials to the rest of the prefix.

**Regression:** `rbitcoin-net` `overlay_misbehavior_discourages_that_net`, `overlay_refusal_ignores_the_port`, `slot_evict_does_not_block_an_outbound_dial`, `feeler_does_not_dial_a_discouraged_address`, `ibd_dial_refuses_a_discouraged_net`.
