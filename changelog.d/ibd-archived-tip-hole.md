Fixed

- **IBD fetches archived bodies above the tip again after a crash.** A crash
  between the Class A write and the tip advance leaves bodies above the tip.
  The restart marked them archived, densify skipped them, and only a
  quarter-full queue raced the tip hole, so IBD could sit at the old tip.
