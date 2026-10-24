Fixed

- **`scriptPubKey` `type` spots bare multisig the way Core does.** It now
  uses the same check as `multi(…)` in `desc`. That check needs every key
  to be a 33- or 65-byte push and needs a key count. It allows up to 20
  keys, and a count of 17 to 20 is a one-byte push. `5101ab51ae` and a
  script with no key count are now `nonstandard`. A 17-key multisig is now
  `multisig`. A hybrid-key multisig stays `multisig`, while its `desc` is
  `raw(…)`, as in Core.
