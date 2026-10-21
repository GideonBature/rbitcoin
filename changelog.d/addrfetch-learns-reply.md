Fixed

- **An addr-fetch session keeps the addresses it asked for.** The `getaddr`
  reply from an addr-fetch peer (the `--proxy` DNS seeds, `-seednode`) was
  dropped before AddrMan and scored as misbehavior. As in Core, the
  `getaddr` now buys `MAX_ADDR_TO_SEND` more tokens, the reply (`addrv2` or
  v1 `addr`) is learned and not relayed, and from 2 through 1,000 addresses
  ends the session with a plain disconnect. More than 1,000 is misbehavior
  and is not learned.
