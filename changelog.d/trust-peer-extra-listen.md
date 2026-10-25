Fixed

- `--trust-peer` rejects an extra P2P listen address the same way it rejects
  `--listen`. A config that names a trust peer and then adds one no longer
  passes startup, and `addnode` of an address fails when no dialer is
  attached instead of remembering that address.
