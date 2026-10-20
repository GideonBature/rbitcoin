Changed

- **Unsafe and bare unwraps are linted.** Crates that do not call FFI forbid `unsafe`. Unsafe blocks need a `// SAFETY:` comment. `unsafe_op_in_unsafe_fn` is denied except the four `loc_simd` helpers. Bare production unwraps are frozen per file in `lint/unwrap-baseline.txt`.
