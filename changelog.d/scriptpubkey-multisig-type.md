Fixed

- **`scriptPubKey` `type` spots bare multisig the way Core does.** Every
  key must now be a 33- or 65-byte public key, and the script needs a
  key count. Up to 20 keys are allowed, and a count of 17 to 20 is a
  one-byte push. `5101ab51ae` and a script with no key count are now
  `nonstandard`. A 17-key multisig is now `multisig`. `multi(…)` in
  `desc` uses the same check and also turns away hybrid keys, so a
  hybrid-key multisig stays `multisig` while its `desc` is `raw(…)`, as
  in Core.
