Changed

- **Stable re-checks now call the single owner.** Unspent creates read one spent range. A failing header batch does not ensure the full batch twice. A soft reject marks the hash missing once. Esplora block summaries use the query median-time window. Verbose `getblockheader` for the active tip, an off-chain header, and a held header share one object builder. Client JSON, HTTP status, and reject strings are unchanged.
