Fixed

- **An Esplora address page no longer joins the whole history.**
  `/address/:addr/txs` and `/txs/summary` take the newest page from the
  posting tail. A full page walks blocks down to that page's height so a
  spend of an older coin is included. A shorter page reads the spend slots
  of the creates it kept. The page is still served when the address has
  more creates than `--max-sh-creates`. Chain stats, balance, and the full
  history still refuse above that cap.
- **An unpaged scripthash join reads spend slots in one batch.**
  Chain stats, balance, listunspent, and Electrum history coalesce
  `spent.body` the way pin reads do. A height-ascending page flushes when
  the next create has no spend slots or those slots leave that group.
