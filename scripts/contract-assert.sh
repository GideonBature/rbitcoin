# Sourced by contract tests. Callers set PASS and FAIL before sourcing.
# shellcheck shell=bash

# Run a command. Print TAP-style ok / not ok and count the result.
assert_ok() {
  local name="$1"
  shift
  if "$@"; then
    echo "ok - $name"
    PASS=$((PASS + 1))
  else
    echo "not ok - $name"
    FAIL=$((FAIL + 1))
  fi
}

# Same counter, with the command's stdout and stderr discarded.
assert_ok_quiet() {
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "ok - $name"
    PASS=$((PASS + 1))
  else
    echo "not ok - $name"
    FAIL=$((FAIL + 1))
  fi
}

# Pass when the command fails.
assert_fail() {
  local name="$1"
  shift
  if "$@"; then
    echo "not ok - $name (expected failure)"
    FAIL=$((FAIL + 1))
  else
    echo "ok - $name"
    PASS=$((PASS + 1))
  fi
}

# Pass when the command fails. Discard its stdout and stderr.
assert_fail_quiet() {
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "not ok - $name (expected failure)"
    FAIL=$((FAIL + 1))
  else
    echo "ok - $name"
    PASS=$((PASS + 1))
  fi
}
