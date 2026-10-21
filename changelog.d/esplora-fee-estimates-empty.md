Fixed

- Off mainnet, Esplora `GET /fee-estimates` is HTTP 200 `{}` when no target has a rate, so a fresh regtest client can apply its own fallback. Mainnet stays HTTP 503 `fee estimates unavailable`. The estimator does not invent a feerate.
