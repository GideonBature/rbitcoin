Fixed

- **Script checks stop once the milestone hash is on the header path.** A block whose hash is that path's hash at its height skips scripts before chain work meets the minimum. A different hash at that height still runs scripts. Prevouts stay on.
