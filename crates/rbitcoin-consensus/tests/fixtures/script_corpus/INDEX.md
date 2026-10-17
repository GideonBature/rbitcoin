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
| `initial-1000-tapscript` | Tapscript accepts an initial stack of 1,000, then 999 DROP leaves one true item. |
| `initial-1001-tapscript` | Tapscript rejects an initial stack of 1,001 before the script runs. |
| `initial-520-tapscript` | Tapscript accepts a 520-byte initial element. |
| `initial-521-tapscript` | Tapscript rejects a 521-byte initial element. |
| `initial-520-v0` | Witness v0 accepts a 520-byte initial element. |
| `initial-521-v0` | Witness v0 rejects a 521-byte initial element. |
| `success-skip-1001` | One OP_SUCCESS accepts before the initial-stack check, including 1,001 items. |
| `success-unexec-1001` | OP_SUCCESS inside an unexecuted IF still accepts before the initial-stack check. |
| `success-unparseable-tail` | OP_SUCCESS followed by a truncated PUSHDATA accepts before the stack check. The unparseable tail is not executed. |
| `success-skip-521` | OP_SUCCESS accepts before the 521-byte initial-element check. |
