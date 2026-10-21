Fixed

- **When the address book is not empty, a seednode is added 10s after RPC is up once an outbound dial has been tried, unless two outbound full-relay peers are already up.** `setmocktime` counts in that window. An empty book adds the seednode as soon as tip-follow is ready. The dial itself is often during catch-up, before the port accepts.
