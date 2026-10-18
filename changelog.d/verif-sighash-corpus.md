Added

- **The script corpus pins executed `OP_VERIF` / `OP_VERNOTIF` and ECDSA hashtypes `0x00`, `0x80`, and `0x85`.** DERSIG still accepts those hashtypes. STRICTENC rejects them. Rewriting the type byte rejects the signature.
