Fixed

- **A BadPrev that races its parent's commit is fetched again.** Load can
  stamp tip+2 while tip+1 is still being written, and that reject can
  reach IBD before tip+1's `Accepted`. IBD then treated the block as a fork,
  dropped it from the path, and never asked for it again, so catch-up
  stalled. A BadPrev whose parent is the path's own block at the height
  below is now fetched again like a tip child.
