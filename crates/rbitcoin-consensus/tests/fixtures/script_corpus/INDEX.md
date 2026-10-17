# Script corpus

Run beside Bitcoin Core's JSON suites. Every row is executed by our
interpreter and by Bitcoin Core v31.1 `VerifyScript` (the submodule, not
the crates.io `bitcoinconsensus` crate, which is Core 26). Both must
agree with the row's expected accept or reject. Error strings are not
compared.

This is not a proof over every byte string. secp256k1 is a shared
library. Block sigop cost, the milestone script skip, and transaction
structure are outside this corpus. Policy flags that confirm never sets
appear only as rows that leave them off.

The `row` array is Core `script_tests.json` form
`[scriptSig, scriptPubKey, flags, result]` and can be appended upstream
without a runner change.

## Vectors

| Id | Predicate |
|----|-----------|
| `smoke-op-1` | Harness: empty scriptSig, `OP_1` scriptPubKey, no flags, accepts on both engines. |
