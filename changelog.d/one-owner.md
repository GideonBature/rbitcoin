Changed

- **Duplicated encoders, mixers, and forwarders now call the single owner.** Class A rows use the named literal `TxRecord { txid, version, locktime, input_count, output_count }`. Body meta still stores version, locktime, and leftover `n_in` only. Consensus results, fee rejects, dial ranking, and client JSON are unchanged.
