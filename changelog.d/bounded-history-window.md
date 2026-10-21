Fixed

- **A confirmed address cursor no longer builds mempool transaction JSON.**
  `GET /address/:addr/txs` and `GET /scripthash/:h/txs` load mempool
  bodies only for rows that stay on the page. A confirmed `after_txid`
  matches `/txs/chain`. A mempool-only cursor past the first 50 rows
  returns that confirmed page instead of 422.
- **A multi-script Esplora page is one newest window per script.**
  `POST /addresses/txs`, `POST /scripthashes/txs`, and the summary twins
  merge those windows into 25 confirmed rows, in the same order the
  next page walks. A later page sends that same cursor to every script.
  A script that contains it continues after it. A script that does not
  keeps rows strictly older than the cursor, still one window of 25.
  One script still rejects a cursor that is not in that script.
- **Electrum `get_history` from a later height scans that window.**
  `from_height` above 0 reads block inputs for `[from, to)` and is served
  above `--max-sh-creates`. A spend of a different output of the same
  transaction is not included. Open history, and a window that starts at
  height 0, still refuse above the cap.
