# 103 — fuse8 segment count must divide the array

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A fuse8 file could name a power-of-two segment length whose segment-count length was not a multiple of that segment. Membership then indexed past the fingerprint array. Opening that geometry returns corrupt. A filter this node writes still loads.

**Regression:** `rbitcoin-store` `fuse8_segment_count_must_be_a_multiple_of_segment_length`.
