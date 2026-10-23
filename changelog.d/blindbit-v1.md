Added

- **BlindBit oracle v1 over HTTP.** With `--sp-tweaks`, the Esplora listener serves `/blindbit/v1/` for silent-payment wallets on spdk's BlindBit v1 backend (Dana): `info`, `block-height`, `block-hash`, `tweaks` and `tweak-index` with `dustLimit`, `utxos`, both BIP158 filters, `spent-index`, and `forward-tx`. Filters and the spent index are built per request from the block; nothing new is stored (#1119).
