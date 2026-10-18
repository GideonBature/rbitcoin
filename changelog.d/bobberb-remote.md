Fixed

- **Orphan promotion walks one worklist.** The mempool walk and the relay
  hub walk both admit a parked chain child then descendants, without one
  stack frame per descendant. A parent published inside that walk, including
  a one-parent package, releases its parked spends on the same list.
- **Electrum history refuses a window wider than 8,064 blocks.** A
  `from_height` of 1 or more still scans that window and does not use
  the scripthash create cap.
- **An Electrum response write uses the idle deadline.** A client that
  stops reading loses the connection. A scripthash status push uses that
  same deadline, and a timeout ends the connection.
- **An SV2 session goes idle after setup.** The setup timeout also
  closes a client that sent one `CoinbaseOutputConstraints` and then
  stopped sending. Tip and fee pushes do not refresh it.
- **A replacement may spend an unconfirmed input only from its one
  direct conflict.** That parent stays a valid child-pays-for-parent.
  Direct-set feerate is unchanged.
- **The invalid-txid set stops at 4,096.** A newer id does not clear
  the ones already stored.
- **Onion and I2P misbehavior is remembered for a day.** The key is
  the overlay identity, not the port. Loopback is still not recorded,
  and nothing is written to `banlist.dat`. An outbound dial, including
  a feeler and an IBD dial, is refused only for that key. A netgroup
  that just lost an inbound slot still cannot take another inbound
  slot, and it does not block dials to the rest of the prefix.
- **`GET /broadcast` is not served.** `POST /tx` remains the admit path.
- **A panicking script-pool chunk releases the wave.** `wait_done`
  returns, and the worker keeps running.
