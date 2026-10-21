Fixed

- **An Esplora address page no longer joins the whole history.**
  `/address/:addr/txs` and `/txs/summary` take the newest page from the
  posting tail. A full page walks those blocks when its oldest create is
  at most 8,064 blocks below the scan height (the tip, or the cursor on a
  later page), so a spend of an older coin is included. A wider gap reads
  the spend slots of that page and of creates below it, and keeps a spender
  only inside the page. A shorter page reads the spend
  slots of the creates it kept. The page is still served when the address has
  more creates than `--max-sh-creates`. Chain stats, balance, and the full
  history still refuse above that cap.
- **An unpaged scripthash join reads spend slots in one batch.**
  Chain stats, balance, listunspent, and Electrum history coalesce
  `spent.body` the way pin reads do. A height-ascending page flushes when
  the next create has no spend slots or those slots leave that group.
