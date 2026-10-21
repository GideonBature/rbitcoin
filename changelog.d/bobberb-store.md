Fixed

- **A fuse8 file whose declared body length does not fit in the file, or whose segment-count length is not a multiple of its segment length, is corrupt on open.**
- **A Class A body read longer than 64 MiB is refused before the buffer is grown.** That includes index writebehind, not only `VarTable`.
- **A scripthash page chain stops after 1,000,000 links.** The free walk also stops when a page offset repeats, so a cycle stays corrupt after the page header is overwritten.
- **Historical body bit-rot and a torn spend page below `spend_durable` are documented limits.** No checksums, and the spend write path is unchanged.
