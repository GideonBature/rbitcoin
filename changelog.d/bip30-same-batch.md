Fixed

- **BIP30 sees a duplicate created earlier in the same confirm batch.**
  A batch's earlier blocks are not connected yet, so the store lookup missed
  their creates. Only reachable below BIP34 (for example regtest with
  `-testactivationheight=bip34@N`).
