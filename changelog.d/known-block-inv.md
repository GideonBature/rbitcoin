Fixed

- **Catch-up exits at the tip when several peers advertise a far-ahead start height.** A block announcement put the announcing peer back on the header walk even when it named a block the node already held. Peers whose `version` start height no chain reaches kept re-arming the lookahead with every announcement, so catch-up never exited and Electrum and Esplora never bound. Only an announced block the node has not seen now puts a peer back on the walk (#1157).
