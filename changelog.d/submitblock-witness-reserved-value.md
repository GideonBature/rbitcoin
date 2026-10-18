Fixed

- **`submitblock` adds the coinbase witness reserved value like Core.**
  A block with a witness commitment and no coinbase witness was rejected
  as `bad-witness-nonce-size`. Core fills in 32 zero bytes once segwit is
  active, and mining software may leave it out.
