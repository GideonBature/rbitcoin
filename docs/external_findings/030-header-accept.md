# 030 — Header accept and chain wipe

**Severity:** critical
**Status:** fixed
**Found by:** coordinated review, 2026-09-21 and 2026-09-22

A received block whose header fails validation is not inserted or
accepted. A non-genesis block with an all-zero previous hash is not held
and does not disconnect the tip. Header work is summed with a checked
add. A zero `nBits` target is not passed to `Header::work`.

Header intake also refuses a new zero-parent non-genesis row, from a
single header, a `headers` batch, or `submitheader`. An unrequested block
with such a header gets the usual misbehavior penalty. Rows written by
older versions stay in the store.

**Regression:** `rbitcoin-net`
`chain::tests::hostile_peer_session`,
`chain::tests::ensure_headers_batch_chain_and_missing_parent`,
`peer::tests::peer_header_dos_and_self_announce`,
`most_work::tests::sum_work_and_work_better`
