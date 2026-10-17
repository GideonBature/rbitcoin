Fixed

- **An Esplora address page no longer joins the whole history.**
  `/address/:addr/txs` and `/txs/summary` take the newest page from the
  posting tail and the blocks down to that page's height. The page is
  still served when the address has more creates than `--max-sh-creates`.
  Chain stats, balance, and the full history still refuse above that cap.
