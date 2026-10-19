Fixed

- **A legacy `addr` in `fc00::/8` is CJDNS, and self-announce drops bogons.**
  The address book stores that prefix as CJDNS when CJDNS is reachable, and
  leaves it out when CJDNS is off.
  `--external-ip` no longer announces CGNAT, documentation, benchmarking,
  class E, Teredo, or `fd00::/8` while discover is on.
- **A newest address page no longer walks the chain.** A page whose creates
  sit more than eight weeks under the tip reads those spend slots, including
  a spend of an older coin that lands on the page.
- **Initial download halts when the block after the tip keeps an unexpected
  previous.** The third fetch stops the node instead of downloading that
  block again.
- **An orphan that still needs another parent keeps the peer that announced
  it.** A disconnect of that peer drops the orphan.
- **A reorg that removes every create for a scripthash waits to publish the
  empty head until the shorter tip is durable.** A crash before that still
  has the creates. A one-key unlink publishes only that key.
- **A package that fails puts a lifted orphan back without peers that
  disconnected while it was out.** If that was the only peer, the orphan
  is dropped. A package that admits the orphan drops that in-flight record.
