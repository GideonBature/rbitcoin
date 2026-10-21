Added

- `GET /block-height/{height}/tx/{index}/out/{vout}` returns a confirmed
  output by short channel id, and still includes the script and value when
  `spent` is true. The same object is on `GET /tx/{txid}/out/{vout}` and
  `POST /block-heights/txs/outs`. `GET /block-height/{height}/tx/{index}`
  matches `GET /tx/{txid}`. All four accept `?asof=`.
