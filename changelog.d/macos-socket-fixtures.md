Fixed

- **Unix socket tests pass under the default macOS `TMPDIR`.** RPC and
  Esplora socket tests bind in a short `TempDir::for_socket` directory, so
  the path fits the 104-byte `sun_path`. The macOS CI smoke now runs them.
