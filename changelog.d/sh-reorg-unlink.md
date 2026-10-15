Fixed

- **A one-block reorg no longer rewrites a large scripthash once per output.**
  Disconnect drops that block's creates from the tail of the key. The previous
  pages stay readable until the new tail is the head, so a full disk cannot
  tear the index.
