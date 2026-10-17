Fixed

- **Script checks stop once the milestone hash is on the header path.** A block whose hash is that path's hash at its height skips scripts before chain work meets the minimum. A different hash at that height still runs scripts. Prevouts stay on.
- **A full pending header map still takes the below-floor ignore.** A batch that does not connect through those pending headers is ignored, with no parent-header walk, until that header path's own work meets the minimum.
- **Peer and IBD connected-checks use the published height index.** A hash the index has not recorded is not connected. That check does not read headers to catch the index up.
