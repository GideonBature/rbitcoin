Fixed

- **`getblocktemplate` proposal mode applies BIP113, BIP68, and the
  version floors.** A time lock is final only below the parent's median
  time, as at connect, not below the proposal's own timestamp. A spend
  whose relative lock is not met is `bad-txns-nonfinal`. A block below the
  BIP34/66/65 `nVersion` floor is `bad-version(0x…)`. Each of these blocks
  used to pass as a proposal, while Core and `submitblock` rejected it.
