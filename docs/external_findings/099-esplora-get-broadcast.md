# 099 — GET /broadcast has no Origin check

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

`GET /broadcast?tx=` admitted a transaction with no Origin check, so a browser page could submit it. The route is gone. `POST /tx` is the admit path.

**Regression:** `rbitcoin-esplora` `esplora_unix_internal`.
