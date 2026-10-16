Fixed

- **The `--max-sh-creates` refusal names the real numbers.** It read
  `(default 10000)` whatever the configured cap was. It now reports the
  scripthash's create count and the cap in force, for example
  `scripthash join exceeds --max-sh-creates (14437 creates, cap 10000)`.
  Esplora still answers 503.
