Fixed

- **The bench converts addresses to Electrum scripthashes again.** Since the
  hex refactor it printed the hash in byte order, so every address target in
  the bench corpora queried a scripthash nobody owns.
