#!/usr/bin/env bash
# Pin the shared contract counters. Does not run the other contract tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PASS=0
FAIL=0
# shellcheck source=contract-assert.sh
source "$ROOT/scripts/contract-assert.sh"
log="$(mktemp)"
cleanup() { rm -f "$log"; }
trap cleanup EXIT

assert_ok "true passes" true >"$log"
[[ "$(cat "$log")" == "ok - true passes" ]]
[[ "$PASS" -eq 1 && "$FAIL" -eq 0 ]]

assert_ok "false fails" false >"$log"
[[ "$(cat "$log")" == "not ok - false fails" ]]
[[ "$PASS" -eq 1 && "$FAIL" -eq 1 ]]

assert_ok_quiet "quiet hides stdout" echo leaked >"$log"
[[ "$(cat "$log")" == "ok - quiet hides stdout" ]]
[[ "$(cat "$log")" != *leaked* ]]
[[ "$PASS" -eq 2 ]]

assert_fail "false is the expected failure" false >"$log"
[[ "$(cat "$log")" == "ok - false is the expected failure" ]]
[[ "$PASS" -eq 3 ]]

assert_fail "true is not a failure" true >"$log"
[[ "$(cat "$log")" == "not ok - true is not a failure (expected failure)" ]]
[[ "$FAIL" -eq 2 ]]

assert_fail_quiet "quiet failure hides stderr" bash -c 'echo leaked >&2; exit 1' >"$log"
[[ "$(cat "$log")" == "ok - quiet failure hides stderr" ]]
[[ "$(cat "$log")" != *leaked* ]]
[[ "$PASS" -eq 4 && "$FAIL" -eq 2 ]]

echo "contract-assert.test.sh: $PASS passed, $FAIL expected failures recorded"
