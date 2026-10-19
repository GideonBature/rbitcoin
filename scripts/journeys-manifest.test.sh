#!/usr/bin/env bash
# The manifest check passes on this tree.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
"$HERE/journeys-manifest.sh"
echo "journeys-manifest.test.sh: ok"
