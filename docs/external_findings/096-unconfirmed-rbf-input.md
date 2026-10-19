# 096 — Replacement may add an unconfirmed input

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A replacement could spend any mempool output that was not itself in the conflict set. Libre Relay keeps BIP125 rule 2 and adds one limit: an unconfirmed input must be a parent of the direct conflicts, and there may be only one direct conflict. A single-conflict CPFP of that parent still replaces, and the direct-set feerate rule is unchanged.

**Regression:** `rbitcoin-mempool` `replacement_unconfirmed_inputs_follow_libre_rule`.
