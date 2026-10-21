Changed

- **The node waits 1s after a changed SV2 coinbase budget.** That wait is `CONSTRAINTS_COOLDOWN`, which the binary passes on `Sv2TpConfig::constraints_cooldown`. A test can pass a shorter duration on the same field.
- **Electrum tweaks subscribe chunk is always 60s.** `ElectrumConfig` no longer has `tweaks_chunk`.
