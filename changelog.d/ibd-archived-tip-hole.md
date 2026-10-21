Fixed

- **IBD reconstructs archived bodies above the tip into the body queue after a crash.** A crash
  between the Class A write and the tip advance leaves bodies above the tip.
  The restart used to mark them archived and skip them, and only a
  quarter-full queue raced the tip hole, so IBD could sit at the old tip.
