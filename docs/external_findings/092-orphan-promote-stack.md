# 092 — Orphan promotion recurses down the parked chain

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

`promote_orphans_of` called itself once per admitted child. A parked chain is bounded by the orphan count cap (3,000), and each frame held that child's transactions. Promotion now walks a queue. The first child is admitted, then its descendants, then the next sibling.

**Regression:** `rbitcoin-mempool` `orphan_chain_promotes_to_the_cluster_cap`.
