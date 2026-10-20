Changed

- `rbitcoin-cli --rpc-url` takes `host:port` (plain HTTP). A scheme, path, or
  userinfo is rejected, including `https://`, which used to be stripped and
  then dialed without TLS.
