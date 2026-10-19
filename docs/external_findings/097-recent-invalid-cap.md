# 097 — Invalid-txid set clears at 4096

**Severity:** low
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

`note_invalid` cleared the whole set at 4,096 entries, so one more invalid txid forgot every earlier one. The set now stops at the cap. A new id is not stored, and the oldest remains. This is the mempool invalid set, separate from the network recent-reject set.

**Regression:** `rbitcoin-mempool` `recent_invalid_keeps_the_oldest`.
