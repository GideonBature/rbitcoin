Fixed

- **`scriptPubKey` `type` spots pay-to-pubkey the way Core does.** It now
  uses the same check as `pk(…)` in `desc`. That check needs the key's
  first byte to match its size: 02 or 03 for a 33-byte key, and 04, 06, or
  07 for a 65-byte key. `2105…ac` and a 65-byte key that starts with 02
  are now `nonstandard`. A hybrid-key (06 or 07) script stays `pubkey`,
  while its `desc` is `raw(…)`, as in Core.
