Fixed

- **OP_TUCK obeys the shared 1,000-item stack limit again, and RIPEMD160 /
  HASH256 reject an over-limit witness v0 stack instead of panicking.**
  Both regressed on master after v0.8.0, when the per-opcode stack check
  was folded into pushes.
