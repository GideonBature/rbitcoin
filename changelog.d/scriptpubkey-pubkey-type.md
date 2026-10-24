Fixed

- **`scriptPubKey` `type` spots pay-to-pubkey the way Core does.** The
  key's first byte must now match its size: 02 or 03 for a 33-byte key,
  and 04, 06, or 07 for a 65-byte key. `2105…ac` and a 65-byte key that
  starts with 02 are now `nonstandard`. `pk(…)` in `desc` uses the same
  check and also turns away hybrid keys (06 or 07), so a hybrid-key
  script stays `pubkey` while its `desc` is `raw(…)`, as in Core.
