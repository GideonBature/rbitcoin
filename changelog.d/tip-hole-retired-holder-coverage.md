Fixed

- **Tip-hole cover no longer counts retired holders as coverage.** The
  tip-hole getdata race retired stalled owners one per pass but kept
  counting them toward the per-hash ask cap. Once every racer on a hole
  hash had been retired (one churn cycle for the `want = 1` contiguous
  holes, four for the tip+1 hash) and the retired owners stayed connected
  without delivering — addr/ping/headers bytes reset the quiet watchdog —
  nothing ever requested the hash again: cover skipped it on
  `holders() >= want` and densify sat at its hole-open zero cap. The
  in-order tip froze with idle peers and empty queues until restart
  (observed: tip pinned six hours mid-IBD). Cover now re-races one fresh
  peer when a hole hash has retired holders but no racer, the same rule
  densify already uses (`inflight_needs_racer`). Retired holders still
  count toward the cap as long as a live racer exists.
