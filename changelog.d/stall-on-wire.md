Fixed

- **IBD disconnects a peer for its own silence.** Getdata must already be on the wire, and that peer's received bytes must have been still for 30 seconds. Tip motion and a later getdata write do not refresh the clock. When every such peer is quiet in the same pass, none are disconnected.
