Fixed

- **A fuse8 file whose declared body length does not fit, or whose segment count does not divide the array, is corrupt on open.**
- **A Class A body read longer than 64 MiB is refused before the buffer is grown.**
- **A scripthash page chain stops after 1,000,000 links.** That covers the read walk and the walk that frees an extent. A cycle returns corrupt.
- **Historical body bit-rot and a torn spend page below `spend_durable` are documented limits.** No checksums, and the spend write path is unchanged.
