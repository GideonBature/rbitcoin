Changed

- **Linux PR CI runs store and query tests on libc pread.** Job `pread` sets `RBITCOIN_IO=pread` and starts with `coverage`, after `qc` and `test`. The `test` job still leaves that variable unset, so it stays on io_uring.
