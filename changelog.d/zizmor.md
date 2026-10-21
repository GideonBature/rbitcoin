Changed

- **Actions scanning is zizmor.** Pull requests, `master`, and the Monday cron audit workflow files with zizmor 1.31.1. The CodeQL analysis workflow is gone. SARIF upload still uses `github/codeql-action/upload-sarif`, and the protected ruleset still names CodeQL until an admin changes it. Rust dependency advisories stay on `cargo deny`.
