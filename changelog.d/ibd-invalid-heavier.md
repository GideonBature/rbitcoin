Fixed

- **IBD asks the header peer for a block no other peer has.** The peer that
  served the headers was never asked for blocks while another peer was
  connected, and a peer that answered `notfound` was asked again for the
  same block. A heavier chain only the header peer had stalled IBD. A peer
  is not asked again for a block it answered `notfound` until it announces
  more.
