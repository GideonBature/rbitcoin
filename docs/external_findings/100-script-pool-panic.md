# 100 — Script-pool panic sticks the wave

**Severity:** medium
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

A chunk incremented `in_wave` and decremented it only after the work returned. A panic skipped that decrement, so `wait_done` never returned and the worker thread died. The count now drops with the chunk, including on unwind, and the worker stays up. The wave finishes as an error.

**Regression:** `rbitcoin-consensus` `chunk_panic_releases_the_wave`.
