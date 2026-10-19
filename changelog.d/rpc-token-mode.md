Changed

- An existing `{datadir}/rpc.token` must be mode 0600 on Unix. Group or world
  bits refuse startup. The node still creates a missing token at 0600 and
  does not chmod a file that is already there. Windows has no mode bits.
