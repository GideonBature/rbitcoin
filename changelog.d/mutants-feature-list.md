Fixed

- **Nightly mutants parse `.cargo/mutants.toml`.** The mutants feature is a list, which cargo-mutants 27.1.0 accepts, so the queue starts instead of dying on a string.
