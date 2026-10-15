Fixed

- **The address book stores only nets this process can dial.** Onion requires `--proxy` or `--onion`, I2P requires `--i2p-sam`, and CJDNS requires `--cjdns-reachable`. `--only-net` drops the other rows, including ones loaded from the peers file.
