Fixed

- **Opening an existing store without `store.secret` fails closed.** The open
  path used the create-time loader, so a missing secret file was replaced with
  a fresh random key. Every stored script/witness would then decode as garbage
  and the keyed `tx.head` probes would miss, with no error at startup. Open now
  refuses a missing secret as corruption; only datadir create generates one.
