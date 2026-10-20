Fixed

- **A header with an all-zero parent is stored only for genesis.** Any
  other such header was written to the header store with no proof-of-work
  or context check, from an unrequested block, a `headers` message, or
  `submitheader`. New rows are now refused, as in Core. An unrequested
  `block` with such a header gets the usual misbehavior penalty, and
  `submitheader` returns `Must submit previous header`.
