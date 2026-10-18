# 107 — A torn spend page below the durable marker stays

**Severity:** medium
**Status:** accepted
**Found by:** bobberb and muse, 2026-10-18

Spend annotate writes `spent.body` with a 4 KiB in-place read-modify-write. `spend_durable` advances about every 10 minutes after `sync_data`. Replay covers only heights above that marker. A torn page can zero a slot already below it. A zero slot reads as unspent, and replay does not heal it. [046](./046-spend-durability.md) fixed the kill-9 gap where the marker advanced without the data sync. This does not change the write path.

**Regression:** none. The limit is [crash recovery](../crash-recovery.md).
