Fixed

- **Script checks stop once the milestone hash is on the header path.** A block whose hash is that path's hash at its height skips scripts before chain work meets the minimum. A different hash at that height still runs scripts. Prevouts stay on.
- **A below-floor header batch is ignored without reading parent headers, including when the pending map is already full.** A batch that does not connect through those pending headers stays on that ignore path until our chain meets minimum chain work.
- **Peer and IBD connected-checks use the published height index.** A hash the index has not recorded is not connected. That check does not read headers to catch the index up.
- **The IBD progress line and block-stall disconnect run off the coordination loop.** The line uses published tip, tx count, queue depths, and the last loop snapshot. A peer is disconnected for a wire stall only after the confirmed tip itself has stopped advancing.
