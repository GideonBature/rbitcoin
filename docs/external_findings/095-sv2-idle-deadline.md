# 095 — SV2 session has no idle deadline after setup

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

The setup deadline covered the handshake and the first `CoinbaseOutputConstraints`. After that frame, `frames.recv` waited with no deadline, so a client that went silent held one of eight session slots. The same setup timeout is now an idle deadline. It moves only when a frame is handled. Tip and fee-check wakes do not refresh it.

**Regression:** `rbitcoin-sv2` `session_silent_after_constraints_is_dropped_at_the_idle_deadline`.
