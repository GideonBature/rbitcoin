Changed

- **Hex failures use hex-conservative's own text.** Electrum, RPC id
  parsing, Esplora broadcast, and `decodescript` no longer rewrite those
  errors into `invalid hex digit`, `odd hex length`, or `must be 32 bytes
  hex`. Esplora still prefixes `invalid hex`. A short non-hex id is the
  library's length error.
