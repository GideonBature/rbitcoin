#!/usr/bin/env bash
# Bare production unwrap ceiling. See lint/unwrap-baseline.txt.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/unwrap-ratchet.py" "$@"
