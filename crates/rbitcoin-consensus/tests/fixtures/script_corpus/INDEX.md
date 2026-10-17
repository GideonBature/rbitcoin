# Script corpus

Run beside Bitcoin Core's JSON suites. Every row is executed by our
interpreter and by Bitcoin Core v31.1 `VerifyScript` (the submodule, not
the crates.io `bitcoinconsensus` crate, which is Core 26). Both must
agree with the row's expected accept or reject. Error strings are not
compared.

This is not a proof over every byte string. secp256k1 is a shared
library. Block sigop cost, the milestone script skip, and transaction
structure are outside this corpus. Policy flags that confirm never sets
appear only as rows that leave them off.

The `row` array is Core `script_tests.json` form
`[scriptSig, scriptPubKey, flags, result]`, or the witness form
`[witness, scriptSig, scriptPubKey, flags, result]`. Either can be
appended upstream without a runner change.

Witness v0 and tapscript rows are spends, so both engines see them
through `VerifyScript`. Legacy rows are bare scripts with no flags.

## Vectors

| Id | Predicate |
|----|-----------|
| `smoke-op-1` | Harness: empty scriptSig, `OP_1` scriptPubKey, no flags, accepts on both engines. |
| `script-len-10000-legacy` | Legacy and witness v0 accept a 10,000-byte script. Tapscript has no script-size cap. |
| `script-len-10000-v0` | Legacy and witness v0 accept a 10,000-byte script. Tapscript has no script-size cap. |
| `script-len-10000-tapscript` | Legacy and witness v0 accept a 10,000-byte script. Tapscript has no script-size cap. |
| `script-len-10001-legacy` | Legacy and witness v0 reject a 10,001-byte script. Tapscript accepts it. |
| `script-len-10001-v0` | Legacy and witness v0 reject a 10,001-byte script. Tapscript accepts it. |
| `script-len-10001-tapscript` | Legacy and witness v0 reject a 10,001-byte script. Tapscript accepts it. |
| `push-520-legacy` | A 520-byte push is the element maximum and accepts. |
| `push-520-v0` | A 520-byte push is the element maximum and accepts. |
| `push-520-tapscript` | A 520-byte push is the element maximum and accepts. |
| `push-521-legacy` | A 521-byte push rejects. |
| `push-521-v0` | A 521-byte push rejects. |
| `push-521-tapscript` | A 521-byte push rejects. |
| `push-520-unexec-legacy` | A 520-byte push inside an unexecuted IF accepts. The element cap still applies there. |
| `push-520-unexec-v0` | A 520-byte push inside an unexecuted IF accepts. The element cap still applies there. |
| `push-520-unexec-tapscript` | A 520-byte push inside an unexecuted IF accepts. The element cap still applies there. |
| `push-521-unexec-legacy` | A 521-byte push inside an unexecuted IF rejects. |
| `push-521-unexec-v0` | A 521-byte push inside an unexecuted IF rejects. |
| `push-521-unexec-tapscript` | A 521-byte push inside an unexecuted IF rejects. |
| `ops-201-legacy` | 201 non-push opcodes accept on legacy and witness v0. Tapscript has no opcode cap. |
| `ops-201-v0` | 201 non-push opcodes accept on legacy and witness v0. Tapscript has no opcode cap. |
| `ops-201-tapscript` | 201 non-push opcodes accept on legacy and witness v0. Tapscript has no opcode cap. |
| `ops-202-legacy` | 202 non-push opcodes reject on legacy and witness v0. Tapscript accepts them. |
| `ops-202-v0` | 202 non-push opcodes reject on legacy and witness v0. Tapscript accepts them. |
| `ops-202-tapscript` | 202 non-push opcodes reject on legacy and witness v0. Tapscript accepts them. |
| `ops-unexec-201-legacy` | Opcodes inside an unexecuted IF count. IF plus 199 NOP plus ENDIF is 201 and accepts. |
| `ops-unexec-201-v0` | Opcodes inside an unexecuted IF count. IF plus 199 NOP plus ENDIF is 201 and accepts. |
| `ops-unexec-201-tapscript` | Opcodes inside an unexecuted IF count. IF plus 199 NOP plus ENDIF is 201 and accepts. |
| `ops-unexec-202-legacy` | IF plus 200 NOP plus ENDIF is 202. Legacy and witness v0 reject. Tapscript accepts. |
| `ops-unexec-202-v0` | IF plus 200 NOP plus ENDIF is 202. Legacy and witness v0 reject. Tapscript accepts. |
| `ops-unexec-202-tapscript` | IF plus 200 NOP plus ENDIF is 202. Legacy and witness v0 reject. Tapscript accepts. |
| `op16-excluded-legacy` | OP_0 through OP_16 do not count toward the 201 opcode cap. 202 OP_16 plus 201 DROP accepts. |
| `op16-excluded-v0` | OP_0 through OP_16 do not count toward the 201 opcode cap. 202 OP_16 plus 201 DROP accepts. |
| `op16-excluded-tapscript` | OP_0 through OP_16 do not count toward the 201 opcode cap. 202 OP_16 plus 201 DROP accepts. |
| `trunc-pushdata1-legacy` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata1-v0` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata1-tapscript` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata2-legacy` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata2-v0` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata2-tapscript` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata4-legacy` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata4-v0` | A truncated PUSHDATA rejects in every signature version. |
| `trunc-pushdata4-tapscript` | A truncated PUSHDATA rejects in every signature version. |
| `else-empty-legacy` | ELSE with an empty condition stack rejects. |
| `else-empty-v0` | ELSE with an empty condition stack rejects. |
| `else-empty-tapscript` | ELSE with an empty condition stack rejects. |
| `endif-empty-legacy` | ENDIF with an empty condition stack rejects. |
| `endif-empty-v0` | ENDIF with an empty condition stack rejects. |
| `endif-empty-tapscript` | ENDIF with an empty condition stack rejects. |
| `if-unclosed-legacy` | An unclosed IF rejects. |
| `if-unclosed-v0` | An unclosed IF rejects. |
| `if-unclosed-tapscript` | An unclosed IF rejects. |
| `verif-unexec-legacy` | OP_VERIF rejects even inside an unexecuted IF, in every signature version. |
| `verif-unexec-v0` | OP_VERIF rejects even inside an unexecuted IF, in every signature version. |
| `verif-unexec-tapscript` | OP_VERIF rejects even inside an unexecuted IF, in every signature version. |
| `vernotif-unexec-legacy` | OP_VERNOTIF rejects even inside an unexecuted IF, in every signature version. |
| `vernotif-unexec-v0` | OP_VERNOTIF rejects even inside an unexecuted IF, in every signature version. |
| `vernotif-unexec-tapscript` | OP_VERNOTIF rejects even inside an unexecuted IF, in every signature version. |
| `stack-1000-legacy` | Legacy main stack of 1,000 accepts. Cleanstack is off. |
| `stack-1001-legacy` | Legacy main stack of 1,001 rejects. |
| `stack-alt-1000-legacy` | Legacy main stack plus altstack of 1,000 accepts: 201 items moved aside, then 799 more. |
| `stack-alt-1001-legacy` | One more push after that shared 1,000 rejects. The opcode budget is already exactly 201. |
| `stack-1001-v0` | Witness v0 rejects a 1,001st stack push. A successful 1,000-deep witness script cannot also return to one element within the 201 opcode budget, so the accept side is the legacy and tapscript rows. |
| `stack-exec-1000-tapscript` | Tapscript enforces the shared 1,000 cap after each opcode. 1,000 pushes and 999 drops leave one true item. |
| `stack-exec-1001-tapscript` | Tapscript rejects the 1,001st push during execution. |
| `disabled-cat-unexec-legacy` | OP_CAT (0x7e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-cat-unexec-v0` | OP_CAT (0x7e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-cat-unexec-tapscript` | OP_CAT (0x7e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-substr-unexec-legacy` | OP_SUBSTR (0x7f) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-substr-unexec-v0` | OP_SUBSTR (0x7f) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-substr-unexec-tapscript` | OP_SUBSTR (0x7f) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-left-unexec-legacy` | OP_LEFT (0x80) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-left-unexec-v0` | OP_LEFT (0x80) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-left-unexec-tapscript` | OP_LEFT (0x80) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-right-unexec-legacy` | OP_RIGHT (0x81) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-right-unexec-v0` | OP_RIGHT (0x81) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-right-unexec-tapscript` | OP_RIGHT (0x81) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-invert-unexec-legacy` | OP_INVERT (0x83) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-invert-unexec-v0` | OP_INVERT (0x83) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-invert-unexec-tapscript` | OP_INVERT (0x83) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-and-unexec-legacy` | OP_AND (0x84) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-and-unexec-v0` | OP_AND (0x84) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-and-unexec-tapscript` | OP_AND (0x84) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-or-unexec-legacy` | OP_OR (0x85) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-or-unexec-v0` | OP_OR (0x85) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-or-unexec-tapscript` | OP_OR (0x85) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-xor-unexec-legacy` | OP_XOR (0x86) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-xor-unexec-v0` | OP_XOR (0x86) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-xor-unexec-tapscript` | OP_XOR (0x86) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2mul-unexec-legacy` | OP_2MUL (0x8d) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2mul-unexec-v0` | OP_2MUL (0x8d) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2mul-unexec-tapscript` | OP_2MUL (0x8d) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2div-unexec-legacy` | OP_2DIV (0x8e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2div-unexec-v0` | OP_2DIV (0x8e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-2div-unexec-tapscript` | OP_2DIV (0x8e) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mul-unexec-legacy` | OP_MUL (0x95) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mul-unexec-v0` | OP_MUL (0x95) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mul-unexec-tapscript` | OP_MUL (0x95) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-div-unexec-legacy` | OP_DIV (0x96) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-div-unexec-v0` | OP_DIV (0x96) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-div-unexec-tapscript` | OP_DIV (0x96) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mod-unexec-legacy` | OP_MOD (0x97) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mod-unexec-v0` | OP_MOD (0x97) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-mod-unexec-tapscript` | OP_MOD (0x97) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-lshift-unexec-legacy` | OP_LSHIFT (0x98) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-lshift-unexec-v0` | OP_LSHIFT (0x98) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-lshift-unexec-tapscript` | OP_LSHIFT (0x98) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-rshift-unexec-legacy` | OP_RSHIFT (0x99) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-rshift-unexec-v0` | OP_RSHIFT (0x99) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `disabled-rshift-unexec-tapscript` | OP_RSHIFT (0x99) rejects in legacy and witness v0 even inside an unexecuted IF. In tapscript that byte is OP_SUCCESS and accepts. |
| `exec-reserved-legacy` | 0x50 (RESERVED) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved-v0` | 0x50 (RESERVED) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved-tapscript` | 0x50 (RESERVED) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-reserved-legacy` | 0x50 (RESERVED) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved-v0` | 0x50 (RESERVED) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved-tapscript` | 0x50 (RESERVED) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-ver-legacy` | 0x62 (VER) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-ver-v0` | 0x62 (VER) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-ver-tapscript` | 0x62 (VER) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-ver-legacy` | 0x62 (VER) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-ver-v0` | 0x62 (VER) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-ver-tapscript` | 0x62 (VER) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-reserved1-legacy` | 0x89 (RESERVED1) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved1-v0` | 0x89 (RESERVED1) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved1-tapscript` | 0x89 (RESERVED1) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-reserved1-legacy` | 0x89 (RESERVED1) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved1-v0` | 0x89 (RESERVED1) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved1-tapscript` | 0x89 (RESERVED1) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-reserved2-legacy` | 0x8a (RESERVED2) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved2-v0` | 0x8a (RESERVED2) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-reserved2-tapscript` | 0x8a (RESERVED2) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-reserved2-legacy` | 0x8a (RESERVED2) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved2-v0` | 0x8a (RESERVED2) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-reserved2-tapscript` | 0x8a (RESERVED2) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-success187-legacy` | 0xbb (SUCCESS187) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-success187-v0` | 0xbb (SUCCESS187) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-success187-tapscript` | 0xbb (SUCCESS187) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-success187-legacy` | 0xbb (SUCCESS187) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-success187-v0` | 0xbb (SUCCESS187) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-success187-tapscript` | 0xbb (SUCCESS187) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-success254-legacy` | 0xfe (SUCCESS254) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-success254-v0` | 0xfe (SUCCESS254) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `exec-success254-tapscript` | 0xfe (SUCCESS254) rejects when executed on legacy and witness v0. Tapscript accepts it as OP_SUCCESS. |
| `unexec-success254-legacy` | 0xfe (SUCCESS254) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-success254-v0` | 0xfe (SUCCESS254) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `unexec-success254-tapscript` | 0xfe (SUCCESS254) is not a legacy disabled opcode, so an unexecuted one is skipped. Tapscript still accepts it as OP_SUCCESS. |
| `exec-return-legacy` | OP_RETURN rejects when executed in every signature version. |
| `exec-return-v0` | OP_RETURN rejects when executed in every signature version. |
| `exec-return-tapscript` | OP_RETURN rejects when executed in every signature version. |
| `unexec-return-legacy` | OP_RETURN inside an unexecuted IF is skipped in every signature version. |
| `unexec-return-v0` | OP_RETURN inside an unexecuted IF is skipped in every signature version. |
| `unexec-return-tapscript` | OP_RETURN inside an unexecuted IF is skipped in every signature version. |
| `exec-unknown-ff-legacy` | Opcode 0xff is not disabled and not OP_SUCCESS. Executing it rejects in every signature version. |
| `exec-unknown-ff-v0` | Opcode 0xff is not disabled and not OP_SUCCESS. Executing it rejects in every signature version. |
| `exec-unknown-ff-tapscript` | Opcode 0xff is not disabled and not OP_SUCCESS. Executing it rejects in every signature version. |
| `unexec-unknown-ff-legacy` | Opcode 0xff inside an unexecuted IF is skipped in every signature version. |
| `unexec-unknown-ff-v0` | Opcode 0xff inside an unexecuted IF is skipped in every signature version. |
| `unexec-unknown-ff-tapscript` | Opcode 0xff inside an unexecuted IF is skipped in every signature version. |
| `success-inside-push-tapscript` | An OP_SUCCESS byte inside a push is data. The following OP_RETURN still rejects. |
| `initial-1000-tapscript` | Tapscript accepts an initial stack of 1,000, then 999 DROP leaves one true item. |
| `initial-1001-tapscript` | Tapscript rejects an initial stack of 1,001 before the script runs. |
| `initial-520-tapscript` | Tapscript accepts a 520-byte initial element. |
| `initial-521-tapscript` | Tapscript rejects a 521-byte initial element. |
| `initial-520-v0` | Witness v0 accepts a 520-byte initial element. |
| `initial-521-v0` | Witness v0 rejects a 521-byte initial element. |
| `success-skip-1001` | One OP_SUCCESS accepts before the initial-stack check, including 1,001 items. |
| `success-unexec-1001` | OP_SUCCESS inside an unexecuted IF still accepts before the initial-stack check. |
| `success-unparseable-tail` | OP_SUCCESS followed by a truncated PUSHDATA accepts before the stack check. The unparseable tail is not executed. |
| `success187-skip-1001` | OP_SUCCESS187 accepts before the 1,001 initial-stack check. |
| `success-skip-521` | OP_SUCCESS accepts before the 521-byte initial-element check. |
| `num-max-roundtrip-legacy` | The 4-byte maximum positive script number decodes, survives ADD 0, and compares equal. Minimal-encoding checks stay off. |
| `num-max-roundtrip-v0` | The 4-byte maximum positive script number decodes, survives ADD 0, and compares equal. Minimal-encoding checks stay off. |
| `num-max-roundtrip-tapscript` | The 4-byte maximum positive script number decodes, survives ADD 0, and compares equal. Minimal-encoding checks stay off. |
| `num-5byte-arith-legacy` | Arithmetic rejects a 5-byte number. CLTV and CSV are the opcodes that allow 5. |
| `num-5byte-arith-v0` | Arithmetic rejects a 5-byte number. CLTV and CSV are the opcodes that allow 5. |
| `num-5byte-arith-tapscript` | Arithmetic rejects a 5-byte number. CLTV and CSV are the opcodes that allow 5. |
| `add-result-may-be-wide-legacy` | ADD of 2147483647 and 1 accepts. The sum does not fit in 4 bytes and stays on the stack for later. |
| `add-result-may-be-wide-v0` | ADD of 2147483647 and 1 accepts. The sum does not fit in 4 bytes and stays on the stack for later. |
| `add-result-may-be-wide-tapscript` | ADD of 2147483647 and 1 accepts. The sum does not fit in 4 bytes and stays on the stack for later. |
| `add-wide-result-not-reused-legacy` | That wide sum rejects when a later arithmetic opcode decodes it. |
| `add-wide-result-not-reused-v0` | That wide sum rejects when a later arithmetic opcode decodes it. |
| `add-wide-result-not-reused-tapscript` | That wide sum rejects when a later arithmetic opcode decodes it. |
| `add-two-max-legacy` | ADD of two 4-byte maximums accepts. The sum is wider than 4 bytes and is not itself an error. |
| `add-two-max-v0` | ADD of two 4-byte maximums accepts. The sum is wider than 4 bytes and is not itself an error. |
| `add-two-max-tapscript` | ADD of two 4-byte maximums accepts. The sum is wider than 4 bytes and is not itself an error. |
| `sub-underflow-wide-legacy` | SUB of 0 minus the 4-byte maximum accepts and leaves a negative wide result. |
| `sub-underflow-wide-v0` | SUB of 0 minus the 4-byte maximum accepts and leaves a negative wide result. |
| `sub-underflow-wide-tapscript` | SUB of 0 minus the 4-byte maximum accepts and leaves a negative wide result. |
| `negate-negzero-legacy` | Four-byte 0x00000080 is negative zero when minimal checks are off. Negating it is 0. |
| `negate-negzero-v0` | Four-byte 0x00000080 is negative zero when minimal checks are off. Negating it is 0. |
| `negate-negzero-tapscript` | Four-byte 0x00000080 is negative zero when minimal checks are off. Negating it is 0. |
| `negate-i32-min-5byte-legacy` | The 5-byte encoding of negative 2^31 is not a 4-byte script number. NEGATE rejects it. |
| `negate-i32-min-5byte-v0` | The 5-byte encoding of negative 2^31 is not a 4-byte script number. NEGATE rejects it. |
| `negate-i32-min-5byte-tapscript` | The 5-byte encoding of negative 2^31 is not a 4-byte script number. NEGATE rejects it. |
| `zero-negzero-numequal-legacy` | 0 and negative zero (0x80) are equal as numbers when minimal encoding is not required. |
| `zero-negzero-numequal-v0` | 0 and negative zero (0x80) are equal as numbers when minimal encoding is not required. |
| `zero-negzero-numequal-tapscript` | 0 and negative zero (0x80) are equal as numbers when minimal encoding is not required. |
| `zero-negzero-equal-legacy` | The same two values are not equal as byte strings under OP_EQUAL. |
| `zero-negzero-equal-v0` | The same two values are not equal as byte strings under OP_EQUAL. |
| `zero-negzero-equal-tapscript` | The same two values are not equal as byte strings under OP_EQUAL. |
| `within-inside-legacy` | WITHIN is inclusive of the low bound. |
| `within-inside-v0` | WITHIN is inclusive of the low bound. |
| `within-inside-tapscript` | WITHIN is inclusive of the low bound. |
| `within-low-legacy` | WITHIN accepts the low bound itself. |
| `within-low-v0` | WITHIN accepts the low bound itself. |
| `within-low-tapscript` | WITHIN accepts the low bound itself. |
| `within-high-legacy` | WITHIN rejects the high bound. |
| `within-high-v0` | WITHIN rejects the high bound. |
| `within-high-tapscript` | WITHIN rejects the high bound. |
| `pick-zero-legacy` | PICK index 0 copies the top item. DROP leaves that one true item. |
| `pick-zero-v0` | PICK index 0 copies the top item. DROP leaves that one true item. |
| `pick-zero-tapscript` | PICK index 0 copies the top item. DROP leaves that one true item. |
| `pick-negative-legacy` | PICK index -1 rejects. |
| `pick-negative-v0` | PICK index -1 rejects. |
| `pick-negative-tapscript` | PICK index -1 rejects. |
| `pick-depth-legacy` | PICK index equal to the remaining stack depth rejects. |
| `pick-depth-v0` | PICK index equal to the remaining stack depth rejects. |
| `pick-depth-tapscript` | PICK index equal to the remaining stack depth rejects. |
| `roll-zero-legacy` | ROLL index 0 leaves the same one item. |
| `roll-zero-v0` | ROLL index 0 leaves the same one item. |
| `roll-zero-tapscript` | ROLL index 0 leaves the same one item. |
| `roll-negative-legacy` | ROLL index -1 rejects. |
| `roll-negative-v0` | ROLL index -1 rejects. |
| `roll-negative-tapscript` | ROLL index -1 rejects. |
| `roll-depth-legacy` | ROLL index equal to the remaining stack depth rejects. |
| `roll-depth-v0` | ROLL index equal to the remaining stack depth rejects. |
| `roll-depth-tapscript` | ROLL index equal to the remaining stack depth rejects. |
| `if-minimal-true-legacy` | IF of a single 0x01 takes the branch. |
| `if-minimal-true-v0` | IF of a single 0x01 takes the branch. |
| `if-minimal-true-tapscript` | IF of a single 0x01 takes the branch. |
| `if-minimal-empty-legacy` | IF of an empty vector is false and is a minimal argument in every signature version. |
| `if-minimal-empty-v0` | IF of an empty vector is false and is a minimal argument in every signature version. |
| `if-minimal-empty-tapscript` | IF of an empty vector is false and is a minimal argument in every signature version. |
| `minimalif-00-legacy` | A bare script ignores MINIMALIF. A single 0x00 is false, the branch is skipped, and the trailing OP_1 accepts. |
| `minimalif-00-v0-off` | Witness v0 accepts a non-minimal IF argument when MINIMALIF is off. |
| `minimalif-00-v0-on` | Witness v0 rejects a single 0x00 IF argument when MINIMALIF is on. |
| `minimalif-00-tapscript` | Tapscript rejects a single 0x00 IF argument even when the MINIMALIF flag is off. |
| `minimalif-negzero-tapscript` | Tapscript rejects negative zero as an IF argument. |
| `minimalif-negzero-v0-off` | Witness v0 accepts negative zero as a false IF argument when MINIMALIF is off. |
| `minimalif-02-legacy` | A bare script with MINIMALIF still accepts OP_2 as an IF argument. |
| `minimalif-02-tapscript` | Tapscript rejects OP_2 as an IF argument. The only minimal true byte is 0x01. |
| `bool-leading-zero-legacy` | cast_to_bool is true when any byte is non-zero, including a leading zero. |
| `bool-leading-zero-v0` | cast_to_bool is true when any byte is non-zero, including a leading zero. |
| `bool-leading-zero-tapscript` | Tapscript MINIMALIF rejects that two-byte true value before cast_to_bool. The only minimal true argument is a single 0x01. |
| `bool-negzero-legacy` | Negative zero is false, so IF takes the ELSE branch. |
| `bool-negzero-v0` | Witness v0 treats negative zero as false when MINIMALIF is off. |
| `unexec-does-not-push-legacy` | An unexecuted IF branch does not push. ELSE leaves the single true item. |
| `unexec-does-not-push-v0` | An unexecuted IF branch does not push. ELSE leaves the single true item. |
| `unexec-does-not-push-tapscript` | An unexecuted IF branch does not push. ELSE leaves the single true item. |
| `minimaldata-unexec-legacy` | MINIMALDATA does not apply inside an unexecuted IF. |
| `minimaldata-exec-legacy` | MINIMALDATA rejects an executed push of 0x01 that is not OP_1. |
| `minimaldata-unexec-v0` | Witness v0 MINIMALDATA does not apply inside an unexecuted IF. |
| `minimaldata-exec-v0` | Witness v0 MINIMALDATA rejects an executed non-minimal push. |
| `unexec-checksigverify-legacy` | CHECKSIGVERIFY in an unexecuted branch does not run. |
| `unexec-checksigverify-v0` | CHECKSIGVERIFY in an unexecuted branch does not run. |
| `unexec-checksigverify-tapscript` | CHECKSIGVERIFY in an unexecuted branch does not run. |
| `exec-checksigverify-empty-legacy` | CHECKSIGVERIFY of an empty signature rejects when the branch runs. |
| `exec-checksigverify-empty-v0` | CHECKSIGVERIFY of an empty signature rejects when the branch runs. |
| `exec-checksigverify-empty-tapscript` | CHECKSIGVERIFY of an empty signature rejects when the branch runs. |
