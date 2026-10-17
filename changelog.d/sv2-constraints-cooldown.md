Changed

- **SV2 budget cooldown is part of the provider config.** `Sv2TpConfig.constraints_cooldown` is how long a changed coinbase budget waits after the previous template. A node still waits 1s. An out-of-tree `Sv2TpConfig` literal has to set the field.
