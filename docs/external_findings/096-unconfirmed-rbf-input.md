# 096 — Replacement may add an unconfirmed input

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A replacement could spend any mempool output that was not itself in the conflict set. An unconfirmed input must be a parent of the direct conflicts. Several of those conflicts may be replaced together, which is what Core's cluster replacement does. Spending a conflict's own output is still refused. A single-conflict CPFP of that parent still replaces, and the direct-set feerate rule is unchanged.

**Regression:** `rbitcoin-mempool` `replacement_unconfirmed_inputs_follow_libre_rule`.
