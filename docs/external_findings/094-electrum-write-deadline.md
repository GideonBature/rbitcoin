# 094 — Electrum writes have no deadline

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

The idle timer wrapped the next read. `write_line` was `write_all` plus `flush`, so a client that stopped reading held one of 256 connection slots for as long as the socket blocked. A response write now uses that same idle deadline and the connection task ends on the timeout. A scripthash status push uses that write too: a timeout ends the task instead of being ignored.

**Regression:** `rbitcoin-electrum` `stalled_electrum_write_is_closed`, `stalled_scripthash_push_ends_the_connection`.
