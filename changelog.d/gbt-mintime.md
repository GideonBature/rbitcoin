Fixed

- **`getblocktemplate` `mintime` is one past the tip's median time.** It was
  the median itself, which a block cannot use (`time-too-old`). Core also
  raises its minimum to the previous block's time minus 600 seconds at
  retarget heights; that floor is not part of this fix. The Esplora
  `GET /block-template` JSON follows.
