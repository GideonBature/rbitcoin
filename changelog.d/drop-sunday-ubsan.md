Fixed

- **Sunday fuzz no longer asks cargo-fuzz for UBSan.** That value is not a cargo-fuzz or rustc sanitizer. `asmap` and `block_wire` stay on the nightly AddressSanitizer run.
