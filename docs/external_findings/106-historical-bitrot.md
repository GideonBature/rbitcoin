# 106 — Historical body bit-rot is an accepted reindex

**Severity:** medium
**Status:** accepted
**Found by:** bobberb and muse, 2026-10-18

Open revalidates Class A only inside the durable tip window. Bodies outside that window are not checked again. `store.secret` xors scripts and witness at rest and mixes head keys. It is not a MAC of the record bodies. A flipped historical byte stays until an operator reindexes. This does not add checksums.

**Regression:** none. The limit is [crash recovery](../crash-recovery.md).
