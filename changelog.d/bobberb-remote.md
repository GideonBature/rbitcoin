Fixed

- **Orphan promotion walks a worklist.** A parked chain is admitted
  child then descendants, without one stack frame per descendant.
- **Electrum history refuses a window wider than 8,064 blocks.** A
  `from_height` of 1 or more still scans that window and does not use
  the scripthash create cap.
- **An Electrum response write uses the idle deadline.** A client that
  stops reading loses the connection instead of holding a slot inside
  `write_all`.
- **An SV2 session goes idle after setup.** The setup timeout also
  closes a client that sent one `CoinbaseOutputConstraints` and then
  stopped sending. Tip and fee pushes do not refresh it.
- **A replacement may spend an unconfirmed input only from its one
  direct conflict.** That parent stays a valid child-pays-for-parent.
  Direct-set feerate is unchanged.
- **The invalid-txid set stops at 4,096.** A newer id does not clear
  the ones already stored.
- **Onion and I2P misbehavior is remembered for a day.** The key is
  the overlay address. Loopback is still not recorded, and nothing is
  written to `banlist.dat`.
- **`GET /broadcast` is not served.** `POST /tx` remains the admit path.
- **A panicking script-pool chunk releases the wave.** `wait_done`
  returns, and the worker keeps running.
