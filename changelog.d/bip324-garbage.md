Changed

- **BIP324 handshakes include random-length garbage, as Core's do.**
  BIP324 lets each side send up to 4095 random "garbage" bytes after its
  public key. The point is that the start of an encrypted connection has
  no fixed length a network observer could match on. rbitcoin sent none,
  so every v2 connection it opened or accepted started with exactly 64
  bytes and finished the handshake in exactly 100 bytes per side. Someone
  who only watches traffic (an ISP, or anything on the path, without
  connecting) could therefore spot rbitcoin's v2 connections and tell an
  rbitcoin node from a Core node, whose openings vary in length. Each
  side now sends 0 to 4095 random bytes, as Core's
  `GenerateRandomGarbage` does. Peers see no other change. Decoy packets
  stay off, as in Core.
