Fixed

- **An empty header queue during IBD asks from the stored tip.** The reply is written onto the queue even when it stops short of the next checkpoint, so block download can resume when a restart kept the walk checkpoint and not the headers in between. A `headers_done` latch that never proved a walk opens again when a peer is ahead. A connecting peer still gets the one initial headers sync.
