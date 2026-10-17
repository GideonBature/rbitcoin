Changed

- **SV2 budget cooldown is always 1s.** A changed coinbase budget waits `CONSTRAINTS_COOLDOWN` after the previous template. `Sv2TpConfig` no longer has a cooldown field.
- **Electrum tweaks subscribe chunk is always 60s.** `ElectrumConfig` no longer has `tweaks_chunk`.
