Fixed

- **Electrum scripthash status is the protocol's `txid:height:` again.** Confirmed rows also hashed the confirming block hash, so Electrum desktop, which recomputes the status from `get_history`, never stored the history of an address with a confirmed transaction and disconnected after its timeout. A same-height replace now keeps the status; `chain_tip` still marks it (#1028).
