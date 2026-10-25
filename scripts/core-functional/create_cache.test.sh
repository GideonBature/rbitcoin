#!/usr/bin/env bash
# Contract pin for create_cache.py (no cargo, no 199-block mine).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PY="$ROOT/scripts/core-functional/create_cache.py"
PASS=0
FAIL=0
source "$ROOT/scripts/contract-assert.sh"
assert_ok() { assert_ok_quiet "$@"; }


WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rbitcoin-cf-cache.XXXXXX")"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

# --preseed-core writes empty blocks/chainstate (Core skip-remine marker).
CORE="$WORKDIR/bitcoin"
assert_ok "preseed-core" python3 "$PY" --preseed-core "$CORE" --cache "$WORKDIR/unused"
if [[ -d "$CORE/test/cache/node0/regtest/blocks" ]] \
  && [[ -d "$CORE/test/cache/node0/regtest/chainstate" ]]; then
  echo "ok - dummy blocks/chainstate exist"
  PASS=$((PASS + 1))
else
  echo "not ok - dummy blocks/chainstate exist"
  FAIL=$((FAIL + 1))
fi

# --ensure is a no-op when HEIGHT+store already look ready.
READY="$WORKDIR/ready"
mkdir -p "$READY/store"
echo x >"$READY/store/keep"
echo 199-mw1-genesis-mock >"$READY/HEIGHT"
if OUT="$(python3 "$PY" --ensure --cache "$READY" 2>&1)" \
  && printf '%s' "$OUT" | grep -q 'already ready' \
  && [[ -f "$READY/store/keep" ]]; then
  echo "ok - --ensure skips a ready cache"
  PASS=$((PASS + 1))
else
  echo "not ok - --ensure skips a ready cache (got: $OUT)"
  FAIL=$((FAIL + 1))
fi

# The proxy answers -28 after setmocktime leaves IBD and before tx relay.
# Cache mining has to wait that out instead of treating the first -28 as fatal.
if python3 - "$PY" <<'PY'
import importlib.util
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

spec = importlib.util.spec_from_file_location("create_cache", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

hits = {"n": 0}

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        n = hits["n"]
        hits["n"] = n + 1
        if n < 2:
            body = (
                b'{"result":null,"error":{"code":-28,"message":"Loading..."},"id":"cache"}'
            )
        else:
            body = b'{"result":0,"error":null,"id":"cache"}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _fmt, *_args):
        return

httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
port = httpd.server_address[1]
threading.Thread(target=httpd.serve_forever, daemon=True).start()
try:
    resp = mod.rpc_until(
        f"http://127.0.0.1:{port}/",
        "__cookie__:x",
        "getblockcount",
        deadline=time.time() + 5,
    )
finally:
    httpd.shutdown()
if resp.get("result") != 0 or hits["n"] != 3:
    print(f"resp={resp!r} hits={hits['n']}")
    sys.exit(1)
PY
then
  echo "ok - rpc_until waits out warmup -28"
  PASS=$((PASS + 1))
else
  echo "not ok - rpc_until waits out warmup -28"
  FAIL=$((FAIL + 1))
fi

echo
echo "$PASS passed, $FAIL failed"
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
