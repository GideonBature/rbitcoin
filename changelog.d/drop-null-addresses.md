Removed

- **Class A rows and spend edges drop unused address links.** `TxRecord` no longer carries `input_start_fk` or `output_start_fk`, and `PointRecord` no longer carries `next`. Schema-17 txout body-meta bytes and Esplora outspend JSON (`spent`, `txid`, `vin`, `status`) are unchanged.
