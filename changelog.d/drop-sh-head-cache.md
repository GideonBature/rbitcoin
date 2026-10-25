Removed

- **Scripthash head cache.** Tip append locates each scripthash head from disk for that batch. The process no longer keeps 65,536 head values between blocks, and `tip: perf` no longer reports `sh_heads`.
