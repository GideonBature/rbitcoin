# 104 — A body read is capped before it allocates

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A create-loc stride is a `u32`. Times eight, that length is about 32 GiB. The body read reserved or zero-filled that buffer before it compared the span to the published end. A read longer than 64 MiB is refused before any buffer grows, on `VarTable` and on the index writebehind reader. A read inside that cap still has to sit inside the published end.

**Regression:** `rbitcoin-store` `body_read_above_the_cap_is_refused`, `index_read_above_the_cap_is_refused`.
