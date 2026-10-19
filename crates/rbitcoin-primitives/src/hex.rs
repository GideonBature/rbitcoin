//! Hex spelling for the workspace.
//!
//! The bytes are [`hex_conservative`](https://docs.rs/hex-conservative), the
//! crate behind `bitcoin::hex`. These functions exist so a call site does
//! not spell `AsRef` or copy the `0x` strip. This crate does not depend on
//! `bitcoin`: store is hashes-only and already depends on primitives.

use hex_conservative::{DisplayHex, FromHex, HexToArrayError};

/// Failed to parse a hex string.
///
/// Display is the Electrum and `decodescript` sentence. Esplora broadcast
/// uses [`HexError::library_sentence`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexError {
    /// Odd number of digits.
    OddLength,
    /// A digit was not hex.
    InvalidDigit,
}

impl HexError {
    /// hex-conservative's sentence, which Esplora returns after `invalid hex:`.
    pub fn library_sentence(self) -> &'static str {
        match self {
            Self::OddLength => "odd length, failed to create bytes from hex",
            Self::InvalidDigit => "invalid char, failed to create bytes from hex",
        }
    }
}

impl std::fmt::Display for HexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OddLength => "odd hex length",
            Self::InvalidDigit => "invalid hex digit",
        })
    }
}

impl std::error::Error for HexError {}

/// Failed to parse Core / Electrum / Esplora display-order 32-byte hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayHashError {
    /// Digit or length problem from [`decode`].
    Hex(HexError),
    /// Even length, and not 32 bytes. `got` is the byte length.
    ///
    /// A short non-hex id (`zz`) lands here. [`FromHex`] for `[u8; 32]`
    /// reports the length before it looks at the digits.
    WrongLength {
        /// Decoded byte length the string would have had.
        got: usize,
    },
}

impl std::fmt::Display for DisplayHashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hex(e) => write!(f, "{e}"),
            Self::WrongLength { got } => {
                write!(f, "hash/txid must be 32 bytes hex (got {got})")
            }
        }
    }
}

impl std::error::Error for DisplayHashError {}

fn strip_0x(s: &str) -> &str {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

fn map_bytes(err: hex_conservative::HexToBytesError) -> HexError {
    match err {
        hex_conservative::HexToBytesError::OddLengthString(_) => HexError::OddLength,
        hex_conservative::HexToBytesError::InvalidChar(_) => HexError::InvalidDigit,
    }
}

/// Lowercase hex encoding of `data`.
#[inline]
pub fn encode(data: impl AsRef<[u8]>) -> String {
    DisplayHex::to_lower_hex_string(data.as_ref())
}

/// Decode a hex string (even length, optional `0x` prefix). Accepts a-f/A-F.
#[inline]
pub fn decode(s: &str) -> Result<Vec<u8>, HexError> {
    Vec::<u8>::from_hex(strip_0x(s)).map_err(map_bytes)
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

/// Parse display-order 32-byte hex → internal byte order.
///
/// Odd length is reported first. Any other length that is not 64 characters
/// is [`DisplayHashError::WrongLength`], including a short non-hex string.
/// `0x` / `0X` is stripped here.
#[inline]
pub fn parse_display_hash32(hex: &str) -> Result<[u8; 32], DisplayHashError> {
    let hex = strip_0x(hex);
    if !hex.len().is_multiple_of(2) {
        return Err(DisplayHashError::Hex(HexError::OddLength));
    }
    match <[u8; 32]>::from_hex(hex) {
        Ok(mut raw) => {
            raw.reverse();
            Ok(raw)
        }
        Err(HexToArrayError::InvalidLength(e)) => {
            Err(DisplayHashError::WrongLength { got: e.invalid / 2 })
        }
        Err(HexToArrayError::InvalidChar(_)) => Err(DisplayHashError::Hex(HexError::InvalidDigit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rejects_bad() {
        assert_eq!(decode("0").unwrap_err().to_string(), "odd hex length");
        assert_eq!(decode("zz").unwrap_err().to_string(), "invalid hex digit");
        assert_eq!(
            decode("zz").unwrap_err().library_sentence(),
            "invalid char, failed to create bytes from hex"
        );
        assert_eq!(
            decode("0").unwrap_err().library_sentence(),
            "odd length, failed to create bytes from hex"
        );
        let _ = &decode("0").unwrap_err() as &dyn std::error::Error;
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
    fn short_non_hex_is_the_length_not_the_digit() {
        assert_eq!(
            parse_display_hash32("c").unwrap_err().to_string(),
            "odd hex length"
        );
        assert_eq!(
            parse_display_hash32("zz").unwrap_err(),
            DisplayHashError::WrongLength { got: 1 }
        );
        assert_eq!(
            parse_display_hash32("abcd").unwrap_err(),
            DisplayHashError::WrongLength { got: 2 }
        );
        let bad = "gg".repeat(32);
        assert_eq!(bad.len(), 64);
        assert_eq!(
            parse_display_hash32(&bad).unwrap_err().to_string(),
            "invalid hex digit"
        );
    }
}
