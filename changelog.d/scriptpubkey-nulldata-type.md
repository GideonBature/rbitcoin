Fixed

- **`scriptPubKey` `type` is `nulldata` only when Core says so.** Core
  needs the bytes after the leading `OP_RETURN` to be push-only: each one
  is a complete data push or an opcode up to `OP_16`, with `OP_RESERVED`
  and `OP_1NEGATE` allowed. Any other opcode after `OP_RETURN`, or a
  truncated push, now gives `nonstandard`. Examples are `6aac`, `6a6a`, and
  `6a4c`. A bare `6a` and a push of any size stay `nulldata`.
