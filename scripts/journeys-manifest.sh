#!/usr/bin/env bash
# `rbitcoin-test` sets autotests = false. A new tests/*.rs that is not a
# module of suite.rs and not its own [[test]] never compiles.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
DIR="$ROOT/crates/rbitcoin-test/tests"
TOML="$ROOT/crates/rbitcoin-test/Cargo.toml"
SUITE="$DIR/suite.rs"
fail=0

for path in "$DIR"/*.rs; do
  base="$(basename "$path")"
  if [[ "$base" == "suite.rs" ]]; then
    continue
  fi
  stem="${base%.rs}"
  if grep -q "mod ${stem};" "$SUITE"; then
    continue
  fi
  if grep -q "name = \"${stem}\"" "$TOML"; then
    continue
  fi
  echo "journeys-manifest: ${base} is not in suite.rs or a [[test]] entry" >&2
  fail=1
done

exit "$fail"
