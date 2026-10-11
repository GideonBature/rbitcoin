Fixed
- **IBD orchestration does not read the header store for the tip.** Confirm publishes the tip hash when the tip moves, and header intake remembers each block's parent, so the main loop can offer blocks and judge forks without waiting on disk.
