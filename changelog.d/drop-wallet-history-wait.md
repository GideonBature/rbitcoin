Removed

- **Electrum binds when the scripthash index covers the tip this node already has.** It no longer waits up to 20 seconds for a follow `getheaders` round. That wait could still open on a history one block short.
