#!/usr/bin/env bash
# Contract: docs/lightning.md Gossip connector states the lookup policy.
# Does not start a node or compile the sample.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DOC="$ROOT/docs/lightning.md"
PASS=0
FAIL=0
source "$ROOT/scripts/contract-assert.sh"

section="$(awk '
  /^## Gossip connector$/ { p = 1 }
  p && /^## / && !/^## Gossip connector$/ { exit }
  p { print }
' "$DOC")"

assert_ok "gossip connector section exists" test -n "$section"
assert_ok "single lookup function" grep -q 'fn lookup_funding_utxo' <<<"$section"
assert_ok "batch lookup function" grep -q 'fn lookup_funding_utxos' <<<"$section"
assert_ok "six confirmations" grep -q 'const CONFIRMATIONS: u32 = 6' <<<"$section"
assert_ok "unknown chain before http" grep -q 'UnknownChain' <<<"$section"
assert_ok "unknown chain skips http" grep -q 'does not send HTTP' <<<"$section"
assert_ok "spent true is unknown tx" grep -q 'spent: true' <<<"$section"
assert_ok "missing output is unknown tx" grep -q '404' <<<"$section"
assert_ok "short confirmations are unknown tx" grep -q 'CONFIRMATIONS' <<<"$section"
assert_ok "server error retries" grep -q 'Retry' <<<"$section"
assert_ok "batch route" grep -q '/block-heights/txs/outs' <<<"$section"
assert_ok "batch cap 300" grep -q 'const BATCH_LIMIT: usize = 300' <<<"$section"
assert_ok "mempool spend uses outspend" grep -q '/tx/{txid}/outspend/{vout}' <<<"$section"

sample="$(awk '
  /^```rust$/ { p = 1; next }
  p && /^```$/ { exit }
  p { print }
' <<<"$section")"
assert_ok "sample get path" grep -q '/block-height/{height}/tx/{index}/out/{vout}' <<<"$sample"
assert_ok "sample get omits asof" bash -c 'grep -q "/block-height/{height}/tx/{index}/out/{vout}" <<<"$1" && ! grep -q asof <<<"$1"' _ "$sample"

if [[ "$FAIL" -ne 0 ]]; then
  echo "ldk-utxo-lookup.test.sh: $PASS passed, $FAIL failed"
  exit 1
fi
echo "ldk-utxo-lookup.test.sh: $PASS passed"
