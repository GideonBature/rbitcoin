Fixed

- **`reconsiderblock` activates the most-work chain, as Core does.** Each
  restored path used to be re-applied on its own, so a chain that only
  outweighed the tip as a whole stayed inactive. A branch that fails to
  connect no longer stops the next one. A restored leaf that loses, and
  was connected before, now stays a valid fork, so a later
  `invalidateblock` can return to it.
