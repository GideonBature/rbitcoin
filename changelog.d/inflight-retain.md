Changed

- **IBD keeps recent in-flight outputs while the accounted heap is under 1 GiB.** A lookup wave that would drop outputs retains 64 more blocks below the normal drop. Over 1 GiB, each wave releases 64 blocks. The line is the sizes-line total (body queue, in-flight outputs, write locations, hash index, height fence, confirm wire, and fuse/MPHF/Class C heap).
