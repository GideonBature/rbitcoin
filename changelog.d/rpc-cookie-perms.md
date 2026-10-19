Changed

- **`--rpc-cookie-perms owner|group|all`** (conf `rpc_cookie_perms=`, NixOS
  `services.rbitcoin.rpc.cookiePerms`). Default `owner` refuses a cookie
  with group or world bits at startup. `group` allows a 0640 file shared
  with another service and refuses group write. `all` allows world read
  (0644) and refuses group or world write. The check is Unix mode bits;
  Windows ignores the flag. The node still does not create or chmod the
  cookie. A group- or world-readable cookie used to warn once and start.
