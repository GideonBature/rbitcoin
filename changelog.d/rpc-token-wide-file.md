Changed

- A Unix `{datadir}/rpc.token` with group or world bits still refuses startup.
  Delete that file and start again. The node mints a new token only when the
  file is gone. chmod to 0600 keeps the secret that was already readable.
