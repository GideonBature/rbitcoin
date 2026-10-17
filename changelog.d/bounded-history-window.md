Fixed

- **A confirmed address cursor no longer builds mempool transaction JSON.**
  `GET /address/:addr/txs` and `GET /scripthash/:h/txs` load mempool
  bodies only for rows that stay on the page. A confirmed `after_txid`
  matches `/txs/chain`.
- **A multi-script Esplora page is one newest window per script.**
  `POST /addresses/txs`, `POST /scripthashes/txs`, and the summary twins
  merge those windows into 25 confirmed rows. A later page walks past
  each script's first 25. One script still rejects a cursor that is not
  in that script.
- **Electrum `get_history` from a later height scans that window.**
  `from_height` above 0 reads block inputs for `[from, to)` and is served
  above `--max-sh-creates`. Open history, and a window that starts at
  height 0, still refuse above the cap.
