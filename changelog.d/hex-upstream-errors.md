Changed

- **Hex failures use hex-conservative's own text.** Electrum, RPC id
  parsing, and `decodescript` no longer rewrite those errors into
  `invalid hex digit`, `odd hex length`, or `must be 32 bytes hex`.
  A short non-hex id is the library's length error.
