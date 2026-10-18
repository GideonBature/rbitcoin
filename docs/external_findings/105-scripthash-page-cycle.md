# 105 — Scripthash page walks stop on a cycle

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A scripthash page chain followed `next` with no stop. Two pages that name each other never returned. The linked walk, the extent tail, and the walk that frees an extent stop after 1,000,000 links, the same bound as a hash-head multi-list, and return corrupt.

**Regression:** `rbitcoin-store` `scripthash_page_cycle_is_corrupt`, `scripthash_page_free_cycle_is_corrupt`.
