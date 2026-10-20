Fixed

- **IBD leaves a header chain whose block fails consensus.** When the most
  work header chain held a consensus-invalid block, IBD rejected the block
  but kept that chain as its candidate. It never downloaded a lighter valid
  chain another peer served, and stayed below the invalid block. The
  candidate now falls back to the invalid block's parent. Headers that build
  on the invalid block are refused, and a heavier valid chain from any peer
  takes over, rewinding confirmed blocks of the dead chain when it forks
  below them.
- **IBD asks the header peer for a block no other peer has.** The peer that
  served the headers was never asked for blocks while another peer was
  connected, and a peer that answered `notfound` was asked again for the
  same block. A heavier chain only the header peer had stalled IBD. A peer
  is not asked again for a block it answered `notfound` until it announces
  more, or for a block above where its announced headers left the download
  path.
