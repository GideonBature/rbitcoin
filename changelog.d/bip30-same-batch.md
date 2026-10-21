Fixed

- **BIP30 sees a duplicate created earlier in the same confirm batch.**
  A batch's earlier blocks are not connected yet, so the store lookup missed
  their creates. The check runs whenever BIP30 is not skipped. Regtest and
  signet never skip it. Mainnet skips it only above the BIP34 height, below
  1,983,702, when the header at the BIP34 height matches.
