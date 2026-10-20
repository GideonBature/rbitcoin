Changed

- `rbitcoin-cli --rpc-url` takes `host:port` (plain HTTP). A scheme, path, or
  userinfo is rejected, including `https://`, which used to be stripped and
  then dialed without TLS. A numeric host must be a strict IP. Leading zeros,
  short quads, and hex forms are rejected so the bearer token is not sent to
  the address `getaddrinfo` would rewrite them into.
