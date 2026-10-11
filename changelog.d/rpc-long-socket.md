Fixed

- A too-long RPC unix socket path no longer drops the TCP listener. `stop`
  still shuts the node down.
