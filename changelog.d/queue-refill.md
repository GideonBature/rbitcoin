Fixed

- **An empty header queue during IBD asks from the stored tip.** A reply that continues that tip is written onto the queue even when it stops short of the next checkpoint, so block download can resume when a restart kept the walk checkpoint and not the headers in between. A reply that misses the checkpoint does not occupy the gap, and a later reply that meets it replaces the short guess. A `headers_done` latch that never proved a walk opens again when a peer is ahead. A proved walk already at the confirmed tip does not. A connecting peer still gets the one initial headers sync.
