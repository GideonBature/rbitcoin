# 102 — fuse8 body length cannot wrap the size check

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

Opening a fuse8 file compared the mapped bytes to `16 +` the declared body length. A length near the top of `u64` overflowed that add and the open panicked. The open returns corrupt when the declared end does not fit in the file, and it does not allocate that length.

**Regression:** `rbitcoin-store` `fuse8_wrapped_body_length_is_corrupt`.
