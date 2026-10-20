Changed

- **One redial, SAM retry, and SV2 budget cooldown in every build.** A debug node used 250ms, 50ms, and 100ms. The binary now uses 2s, 500ms, and 1s, and it no longer warns about short test timings. A test that waits on one of those sleeps passes the short duration in, so the suite does not get slower. Shutdown still wakes on the stop signal and the 750ms grace.
