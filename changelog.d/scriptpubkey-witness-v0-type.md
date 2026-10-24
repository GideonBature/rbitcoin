Fixed

- **`scriptPubKey` `type` is `nonstandard` for a version 0 witness
  program that is neither 20 nor 32 bytes, as in Core.** A version 0
  program is a key hash at 20 bytes and a script hash at 32. Any other
  length, such as `0003abcdef`, was `witness_unknown`. It is now
  `nonstandard`. Versions 1 to 16 that are not taproot or an anchor stay
  `witness_unknown`.
