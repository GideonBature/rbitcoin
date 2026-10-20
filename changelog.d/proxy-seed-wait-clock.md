Changed

- **The proxy-seed wait test pauses its clock.** The 400 ms deadline and the 250 ms poll still order the events. The node suite no longer spends that half-second.
