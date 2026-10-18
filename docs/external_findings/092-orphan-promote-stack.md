# 092 — Orphan promotion recurses down the parked chain

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

`promote_orphans_of` called itself once per admitted child. The relay hub did the same: `promote_orphans_staged` called `accept_with_utxo`, and a successful admit called `promote_orphans_staged` again. A parked chain is bounded by the orphan count cap (3,000), and each frame held that child's admit. Both walks are queues. The first child is admitted, then its descendants, then the next sibling. An admit inside the hub walk does not start a second walk.

**Regression:** `rbitcoin-mempool` `orphan_chain_promotes_to_the_cluster_cap`. `rbitcoin-net` `parked_chain_promotes_in_one_frame`.
