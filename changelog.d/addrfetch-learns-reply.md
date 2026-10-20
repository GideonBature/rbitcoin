Fixed

- **An addr-fetch session keeps the addresses it asked for.** The `getaddr`
  reply from an addr-fetch peer (the `--proxy` DNS seeds, `-seednode`) was
  dropped before AddrMan and scored as misbehavior. As in Core, the
  `getaddr` now buys `MAX_ADDR_TO_SEND` more tokens, the reply is learned,
  and the session ends with a plain disconnect.
