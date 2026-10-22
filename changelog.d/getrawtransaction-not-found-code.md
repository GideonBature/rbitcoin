Fixed

- **`getrawtransaction` for an unknown txid is error `-5`.** It was `-1`.
  Core answers `-5` (`RPC_INVALID_ADDRESS_OR_KEY`), and clients match that
  code to tell a missing transaction from a node failure.
