Fixed

- **BIP30 turns off inside the write that contains the BIP34 block.**
  The skip used to wait until that block was committed. A later block in
  the same write still ran BIP30. One-block connect already skipped.
