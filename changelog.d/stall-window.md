Fixed

- **An IBD stall window starts when its first getdata is written and ends when that peer owes nothing.** Time spent idle between batches no longer counts against the next ask. A later getdata while a hash from that window is still outstanding does not move the deadline.
