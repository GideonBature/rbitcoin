#!/usr/bin/env bash
# Contract: bare production unwraps are frozen per file.
# Fixtures under lint/unwrap-ratchet/fixtures. Does not scan the repo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PY="$ROOT/scripts/unwrap-ratchet.py"
FIX="$ROOT/lint/unwrap-ratchet/fixtures"
PASS=0
FAIL=0
source "$ROOT/scripts/contract-assert.sh"

assert_fail "bare unwrap in src fails an empty baseline" \
  python3 "$PY" --root "$FIX/bare"
assert_ok "cfg(test) module unwrap is not counted" \
  python3 "$PY" --root "$FIX/cfg-test"
assert_ok "lock and try_into unwraps are not counted" \
  python3 "$PY" --root "$FIX/excluded"
assert_fail "count above the baseline fails" \
  python3 "$PY" --root "$FIX/over"
assert_fail "count below the baseline fails" \
  python3 "$PY" --root "$FIX/under"
assert_ok "a matching count passes" \
  python3 "$PY" --root "$FIX/match"

if [[ "$FAIL" -ne 0 ]]; then
  echo "unwrap-ratchet.test.sh: $PASS passed, $FAIL failed"
  exit 1
fi
echo "unwrap-ratchet.test.sh: $PASS passed"
