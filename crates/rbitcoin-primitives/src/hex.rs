//! Hex spelling for the workspace.
//!
//! The bytes are [`hex_conservative`](https://docs.rs/hex-conservative), the
//! crate behind `bitcoin::hex`. These functions exist so a call site does
//! not spell `AsRef`, and so `0x` stripping and display-order reversal live
//! in one place. This crate does not depend on `bitcoin`: store is
//! hashes-only and already depends on primitives.
//!
//! Errors are hex-conservative's own.

use bitcoin_hashes::{sha256, Hash};
use hex_conservative::{DisplayHex, FromHex};

pub use hex_conservative::{HexToArrayError, HexToBytesError};

fn strip_0x(s: &str) -> &str {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

/// Lowercase hex encoding of `data`.
#[inline]
pub fn encode(data: impl AsRef<[u8]>) -> String {
    DisplayHex::to_lower_hex_string(data.as_ref())
}

/// Decode a hex string (even length, optional `0x` prefix). Accepts a-f/A-F.
#[inline]
pub fn decode(s: &str) -> Result<Vec<u8>, HexToBytesError> {
    Vec::<u8>::from_hex(strip_0x(s))
}

/// Core / Electrum / Esplora **display order** hex for a 32-byte hash or txid.
///
/// Store and rust-bitcoin `to_byte_array()` use **internal** byte order; RPC
/// clients expect the reversed hex (same as `BlockHash`/`Txid` `Display`).
#[inline]
pub fn display_hash_hex(mut h: [u8; 32]) -> String {
    h.reverse();
    encode(h)
}

/// Electrum / Esplora scripthash: SHA256(scriptPubKey) in display order.
///
/// `sha256::Hash`'s `Display` prints internal byte order. `Txid` display
/// does not. This is the spelling a client sends.
#[inline]
pub fn scripthash_display_hex(script: &[u8]) -> String {
    display_hash_hex(sha256::Hash::hash(script).to_byte_array())
}

/// Parse display-order 32-byte hex → internal byte order.
///
/// `0x` / `0X` is stripped here. Any other failure is [`HexToArrayError`]:
/// a length other than 64 characters, including a short non-hex id, is
/// reported as a length error before the digits are read.
#[inline]
pub fn parse_display_hash32(hex: &str) -> Result<[u8; 32], HexToArrayError> {
    let mut raw = <[u8; 32]>::from_hex(strip_0x(hex))?;
    raw.reverse();
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin_hashes::{sha256, Hash};

    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 0xab, 0xff];
        assert_eq!(encode(data), "0001abff");
        assert_eq!(decode("0001abff").unwrap(), data);
        assert_eq!(decode("0001ABFF").unwrap(), data);
        assert_eq!(decode("0x0a").unwrap(), vec![0x0a]);
        assert_eq!(decode("0Xff").unwrap(), vec![0xff]);
        assert_eq!(encode([]), "");
    }

    #[test]
    fn rejects_bad_with_the_upstream_error() {
        assert_eq!(
            decode("0").unwrap_err().to_string(),
            <Vec<u8> as FromHex>::from_hex("0").unwrap_err().to_string()
        );
        assert_eq!(
            decode("zz").unwrap_err().to_string(),
            <Vec<u8> as FromHex>::from_hex("zz")
                .unwrap_err()
                .to_string()
        );
    }

    #[test]
    fn p2a_scripthash_display_is_reversed_sha256_not_hash_display() {
        // bc1pfeessrawgf. sha256::Hash Display is internal order.
        let spk = [0x51, 0x02, 0x4e, 0x73];
        let got = scripthash_display_hex(&spk);
        assert_eq!(
            got,
            "c5d0fb3863474a90cfe5b26801e66cb45fdc046b40e6829788eaf1b8770ffea6"
        );
        let digest = sha256::Hash::hash(&spk);
        assert_ne!(got, digest.to_string());
        assert_eq!(got, display_hash_hex(digest.to_byte_array()));
    }

    #[test]
    fn display_hash_roundtrips_known_internal_bytes() {
        let mut internal = [0u8; 32];
        internal[0] = 0xab;
        internal[31] = 0xcd;
        let display = "cd000000000000000000000000000000000000000000000000000000000000ab";
        assert_eq!(display_hash_hex(internal), display);
        assert_eq!(parse_display_hash32(display).unwrap(), internal);
        assert_eq!(
            parse_display_hash32(&format!("0x{display}")).unwrap(),
            internal
        );
    }

    #[test]
    fn id_parse_returns_the_upstream_array_error() {
        assert_eq!(
            parse_display_hash32("zz").unwrap_err().to_string(),
            <[u8; 32] as FromHex>::from_hex("zz")
                .unwrap_err()
                .to_string()
        );
        assert_eq!(
            parse_display_hash32("c").unwrap_err().to_string(),
            <[u8; 32] as FromHex>::from_hex("c")
                .unwrap_err()
                .to_string()
        );
        let bad = "gg".repeat(32);
        assert_eq!(
            parse_display_hash32(&bad).unwrap_err().to_string(),
            <[u8; 32] as FromHex>::from_hex(&bad)
                .unwrap_err()
                .to_string()
        );
    }
}
