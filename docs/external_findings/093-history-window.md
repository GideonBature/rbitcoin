# 093 — History catch-up scans every block in the window

**Severity:** high
**Status:** fixed
**Found by:** bobberb and muse, 2026-10-18

`get_history` with `from_height` of 0 is the capped scripthash join. `from_height` of 1 or more is a catch-up: it loads the posting and reads `input.body` for every block in the clipped window, and it does not consult `--max-sh-creates`. A window wider than 8,064 blocks is refused before that scan. A one-block catch-up still returns the spend.

**Regression:** `rbitcoin-query` `history_window_wider_than_eight_weeks_is_refused`.
