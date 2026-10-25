Removed

- **Scripthash head cache.** Tip append locates each scripthash head from disk for that batch. The process no longer keeps 65,536 head values between blocks. `tip: perf` and the `owned` object on `ibd: perf` no longer report `sh_heads`.
