//! Core-compatible asmap bytecode (`src/util/asmap.cpp`).
//!
//! Bits in the file are little-endian (LSB of each byte first). IP bits are
//! consumed most-significant-first. IPv4 lookups use the IPv4-mapped prefix
//! `::ffff:0:0/96` then 32 IPv4 bits (Core `GetMappedAS`).

use std::net::IpAddr;
use std::path::Path;

const INVALID: u32 = 0xFFFFFFFF;

const TYPE_BIT_SIZES: [u8; 3] = [0, 0, 1];
const ASN_BIT_SIZES: [u8; 10] = [15, 16, 17, 18, 19, 20, 21, 22, 23, 24];
const MATCH_BIT_SIZES: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const JUMP_BIT_SIZES: [u8; 26] = [
    5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
    30,
];

const RETURN: u32 = 0;
const JUMP: u32 = 1;
const MATCH: u32 = 2;
const DEFAULT: u32 = 3;

/// Validated asmap bytecode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsMap {
    bytes: Vec<u8>,
}

impl AsMap {
    /// `None` when the bytes fail Core `SanityCheckAsmap` (128-bit inputs).
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        if !sanity_check(&bytes) {
            return None;
        }
        Some(Self { bytes })
    }

    pub fn from_path(path: &Path) -> std::io::Result<Option<Self>> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_bytes(bytes))
    }

    // An empty file fails sanity, so a loaded map is never empty.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn interpret_ip16(&self, ip16: &[u8; 16]) -> u32 {
        interpret(&self.bytes, ip16)
    }

    pub fn mapped_as(&self, ip: IpAddr) -> u32 {
        interpret(&self.bytes, &ip16_for_lookup(ip))
    }

    pub fn digest_hex8(&self) -> String {
        use bitcoin::hashes::{sha256, Hash};
        let hex = sha256::Hash::hash(&self.bytes).to_string();
        hex.chars().take(8).collect()
    }
}

/// Two-prefix DecodeAsmap fixture: IPv4 `1.2.0.0/16` → ASN 1, `3.4.0.0/16` → ASN 2.
pub const TWO_PREFIX_ASMAP: &[u8] = &[
    0xfb, 0x03, 0xec, 0x0f, 0xb0, 0x3f, 0xc0, 0xfe, 0x00, 0xfb, 0x03, 0xec, 0x0f, 0xb0, 0x3f, 0xc0,
    0xfe, 0x00, 0xfb, 0x03, 0xec, 0x0f, 0xb0, 0xff, 0xff, 0xfe, 0xff, 0xfb, 0x80, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01,
];

/// IPv4 as `::ffff:0:0/96` + 32 bits; IPv6 as 16 raw bytes.
pub fn ip16_for_lookup(ip: IpAddr) -> [u8; 16] {
    match ip {
        IpAddr::V4(v4) => {
            let mut a = [0u8; 16];
            a[10] = 0xff;
            a[11] = 0xff;
            a[12..16].copy_from_slice(&v4.octets());
            a
        }
        IpAddr::V6(v6) => v6.octets(),
    }
}

fn consume_bit_le(bitpos: &mut usize, data: &[u8]) -> Option<bool> {
    if *bitpos >= data.len().saturating_mul(8) {
        return None;
    }
    let b = (data[*bitpos / 8] >> (*bitpos % 8)) & 1 == 1;
    *bitpos += 1;
    Some(b)
}

fn consume_bit_be(bitpos: &mut usize, data: &[u8]) -> Option<bool> {
    if *bitpos >= data.len().saturating_mul(8) {
        return None;
    }
    let b = (data[*bitpos / 8] >> (7 - (*bitpos % 8))) & 1 == 1;
    *bitpos += 1;
    Some(b)
}

fn decode_bits(bitpos: &mut usize, data: &[u8], minval: u32, bit_sizes: &[u8]) -> u32 {
    let mut val = minval;
    for (i, &sz) in bit_sizes.iter().enumerate() {
        let last = i + 1 == bit_sizes.len();
        let bit = if last {
            false
        } else {
            match consume_bit_le(bitpos, data) {
                Some(b) => b,
                None => break,
            }
        };
        if bit {
            val = val.saturating_add(1u32 << sz);
        } else {
            for b in 0..sz {
                let Some(bit) = consume_bit_le(bitpos, data) else {
                    return INVALID;
                };
                if bit {
                    val = val.saturating_add(1u32 << (sz - 1 - b));
                }
            }
            return val;
        }
    }
    INVALID
}

fn decode_type(bitpos: &mut usize, data: &[u8]) -> u32 {
    decode_bits(bitpos, data, 0, &TYPE_BIT_SIZES)
}

fn decode_asn(bitpos: &mut usize, data: &[u8]) -> u32 {
    decode_bits(bitpos, data, 1, &ASN_BIT_SIZES)
}

fn decode_match(bitpos: &mut usize, data: &[u8]) -> u32 {
    decode_bits(bitpos, data, 2, &MATCH_BIT_SIZES)
}

fn decode_jump(bitpos: &mut usize, data: &[u8]) -> u32 {
    decode_bits(bitpos, data, 17, &JUMP_BIT_SIZES)
}

fn bit_width(x: u32) -> u32 {
    32 - x.leading_zeros()
}

/// Core `Interpret`. Aborts return `0` (never panic).
pub fn interpret(asmap: &[u8], ip16: &[u8; 16]) -> u32 {
    let mut pos = 0usize;
    let endpos = asmap.len().saturating_mul(8);
    let mut ip_bit = 0usize;
    let ip_bits_end = 128usize;
    let mut default_asn = 0u32;
    while pos < endpos {
        let opcode = decode_type(&mut pos, asmap);
        if opcode == RETURN {
            let asn = decode_asn(&mut pos, asmap);
            if asn == INVALID {
                break;
            }
            return asn;
        } else if opcode == JUMP {
            let jump = decode_jump(&mut pos, asmap);
            if jump == INVALID {
                break;
            }
            if ip_bit == ip_bits_end {
                break;
            }
            let Some(bit) = consume_bit_be(&mut ip_bit, ip16) else {
                break;
            };
            if bit {
                pos += jump as usize;
            }
        } else if opcode == MATCH {
            let matchv = decode_match(&mut pos, asmap);
            if matchv == INVALID {
                break;
            }
            let matchlen = bit_width(matchv) as i32 - 1;
            if matchlen <= 0 {
                break;
            }
            let matchlen = matchlen as usize;
            if ip_bits_end.saturating_sub(ip_bit) < matchlen {
                break;
            }
            let mut ok = true;
            for bit in 0..matchlen {
                let Some(got) = consume_bit_be(&mut ip_bit, ip16) else {
                    ok = false;
                    break;
                };
                let want = ((matchv >> (matchlen - 1 - bit)) & 1) == 1;
                if got != want {
                    return default_asn;
                }
            }
            if !ok {
                break;
            }
        } else if opcode == DEFAULT {
            default_asn = decode_asn(&mut pos, asmap);
            if default_asn == INVALID {
                break;
            }
        } else {
            break;
        }
    }
    0
}

/// Core `SanityCheckAsmap(data, 128)`.
pub fn sanity_check(asmap: &[u8]) -> bool {
    sanity_check_bits(asmap, 128)
}

fn sanity_return(
    pos: &mut usize,
    asmap: &[u8],
    endpos: usize,
    prevopcode: u32,
    jumps: &mut Vec<(usize, i32)>,
    bits: &mut i32,
) -> Option<bool> {
    if prevopcode == DEFAULT {
        return Some(false);
    }
    let asn = decode_asn(pos, asmap);
    if asn == INVALID {
        return Some(false);
    }
    if jumps.is_empty() {
        if endpos - *pos > 7 {
            return Some(false);
        }
        while *pos != endpos {
            match consume_bit_le(pos, asmap) {
                Some(true) => return Some(false),
                Some(false) => {}
                None => return Some(false),
            }
        }
        return Some(true);
    }
    let Some((target, restore)) = jumps.pop() else {
        return Some(false);
    };
    if *pos != target {
        return Some(false);
    }
    *bits = restore;
    None
}

fn sanity_jump(
    pos: &mut usize,
    asmap: &[u8],
    bits: &mut i32,
    jumps: &mut Vec<(usize, i32)>,
) -> bool {
    let jump = decode_jump(pos, asmap);
    if jump == INVALID {
        return false;
    }
    // A jump to EOF or past it cannot be popped onto a finishing RETURN
    // (the target compare fails, or the walker falls off and returns false).
    if *bits == 0 {
        return false;
    }
    *bits -= 1;
    let jump_offset = *pos + jump as usize;
    if let Some(&(prev, _)) = jumps.last() {
        if jump_offset >= prev {
            return false;
        }
    }
    jumps.push((jump_offset, *bits));
    true
}

fn sanity_match(
    pos: &mut usize,
    asmap: &[u8],
    prevopcode: u32,
    had_incomplete_match: &mut bool,
    bits: &mut i32,
) -> bool {
    let matchv = decode_match(pos, asmap);
    if matchv == INVALID {
        return false;
    }
    let matchlen = bit_width(matchv) as i32 - 1;
    if prevopcode != MATCH {
        *had_incomplete_match = false;
    }
    if matchlen < 8 && *had_incomplete_match {
        return false;
    }
    *had_incomplete_match = matchlen < 8;
    if *bits < matchlen {
        return false;
    }
    *bits -= matchlen;
    true
}

fn sanity_check_bits(asmap: &[u8], mut bits: i32) -> bool {
    let mut pos = 0usize;
    let endpos = asmap.len().saturating_mul(8);
    let mut jumps: Vec<(usize, i32)> = Vec::new();
    let mut prevopcode = JUMP;
    let mut had_incomplete_match = false;

    while pos != endpos {
        if jumps.last().is_some_and(|&(p, _)| pos >= p) {
            return false;
        }

        let opcode = decode_type(&mut pos, asmap);
        if opcode == RETURN {
            match sanity_return(&mut pos, asmap, endpos, prevopcode, &mut jumps, &mut bits) {
                Some(done) => return done,
                None => prevopcode = JUMP,
            }
        } else if opcode == JUMP {
            if !sanity_jump(&mut pos, asmap, &mut bits, &mut jumps) {
                return false;
            }
            prevopcode = JUMP;
        } else if opcode == MATCH {
            if !sanity_match(
                &mut pos,
                asmap,
                prevopcode,
                &mut had_incomplete_match,
                &mut bits,
            ) {
                return false;
            }
            prevopcode = MATCH;
        } else if opcode == DEFAULT {
            if prevopcode == DEFAULT {
                return false;
            }
            let asn = decode_asn(&mut pos, asmap);
            if asn == INVALID {
                return false;
            }
            prevopcode = DEFAULT;
        } else {
            return false;
        }
    }
    false
}

#[cfg(test)]
pub(crate) fn two_prefix_asmap_bytes() -> Vec<u8> {
    encode_two_prefix_asmap()
}

#[cfg(test)]
fn encode_bits(val: u32, minval: u32, bit_sizes: &[u8]) -> Vec<bool> {
    let mut x = val - minval;
    let mut out = Vec::new();
    for (i, &sz) in bit_sizes.iter().enumerate() {
        let last = i + 1 == bit_sizes.len();
        let span = 1u32 << sz;
        if !last {
            if x >= span {
                out.push(true);
                x -= span;
                continue;
            }
            out.push(false);
            for b in (0..sz).rev() {
                out.push((x >> b) & 1 == 1);
            }
            return out;
        }
        for b in (0..sz).rev() {
            out.push((x >> b) & 1 == 1);
        }
        return out;
    }
    out
}

#[cfg(test)]
fn pack_le(bits: &[bool]) -> Vec<u8> {
    if bits.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u8; bits.len().div_ceil(8)];
    for (i, &b) in bits.iter().enumerate() {
        if b {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

#[cfg(test)]
fn instr_type(t: u32) -> Vec<bool> {
    encode_bits(t, 0, &TYPE_BIT_SIZES)
}

#[cfg(test)]
fn instr_return(asn: u32) -> Vec<bool> {
    let mut out = instr_type(RETURN);
    out.extend(encode_bits(asn, 1, &ASN_BIT_SIZES));
    out
}

#[cfg(test)]
fn instr_jump(offset: u32) -> Vec<bool> {
    let mut out = instr_type(JUMP);
    out.extend(encode_bits(offset, 17, &JUMP_BIT_SIZES));
    out
}

#[cfg(test)]
fn instr_match(pattern: &[bool]) -> Vec<bool> {
    let n = pattern.len();
    assert!((1..=8).contains(&n));
    let mut matchv = 1u32 << n;
    for (i, &b) in pattern.iter().enumerate() {
        if b {
            matchv |= 1 << (n - 1 - i);
        }
    }
    let mut out = instr_type(MATCH);
    out.extend(encode_bits(matchv, 2, &MATCH_BIT_SIZES));
    out
}

#[cfg(test)]
fn encode_two_prefix_asmap() -> Vec<u8> {
    let mut bits = Vec::new();
    for _ in 0..10 {
        bits.extend(instr_match(&[false; 8]));
    }
    for _ in 0..2 {
        bits.extend(instr_match(&[true; 8]));
    }
    bits.extend(instr_match(&[false; 6]));
    let left = instr_return(1);
    assert_eq!(left.len(), 17);
    bits.extend(instr_jump(left.len() as u32));
    bits.extend(left);
    bits.extend(instr_return(2));
    pack_le(&bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn asmap_two_prefix_maps_distinct_asns() {
        let bytes = two_prefix_asmap_bytes();
        let m = AsMap::from_bytes(bytes).expect("fixture sanity");
        assert_eq!(m.len(), 35);
        assert_ne!(m.len(), 0);
        assert_ne!(m.len(), 1);
        // sha256(TWO_PREFIX_ASMAP) prefix, not a second call of digest_hex8.
        assert_eq!(m.digest_hex8(), "070965c4");
        assert_eq!(m.mapped_as(IpAddr::V4(Ipv4Addr::new(1, 2, 0, 0))), 1);
        assert_eq!(m.mapped_as(IpAddr::V4(Ipv4Addr::new(3, 4, 0, 0))), 2);
        assert_eq!(m.mapped_as(IpAddr::V4(Ipv4Addr::new(1, 2, 9, 9))), 1);
        let mapped = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x0102, 0x0304));
        assert_eq!(m.mapped_as(mapped), 1);
        assert_eq!(m.mapped_as(IpAddr::V6(Ipv6Addr::LOCALHOST)), 0);
    }

    #[test]
    fn asmap_truncated_file_is_none() {
        assert!(AsMap::from_bytes(Vec::new()).is_none());
        assert!(AsMap::from_bytes(vec![0]).is_none());
        let dir = std::env::temp_dir().join(format!(
            "rbitcoin-asmap-trunc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.dat");
        std::fs::write(&path, [0u8]).unwrap();
        assert!(AsMap::from_path(&path).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn asmap_interpret_garbage_returns_zero_no_panic() {
        assert_eq!(interpret(&[], &[0u8; 16]), 0);
        assert_eq!(interpret(&[0xff, 0xff], &[0u8; 16]), 0);
        assert_eq!(interpret(&[0], &[0u8; 16]), 0);
    }

    #[test]
    fn asmap_lone_return_asn1() {
        let bits = instr_return(1);
        let bytes = pack_le(&bits);
        let m = AsMap::from_bytes(bytes).expect("RETURN ASN1");
        assert_eq!(m.len(), 3);
        assert_eq!(m.digest_hex8(), "709e80c8");
        assert_eq!(m.interpret_ip16(&[0u8; 16]), 1);
        assert_eq!(m.interpret_ip16(&[0xff; 16]), 1);
    }

    #[test]
    fn asmap_two_prefix_const_matches_encoder() {
        assert_eq!(two_prefix_asmap_bytes(), TWO_PREFIX_ASMAP);
    }

    #[test]
    fn asmap_fuzz_seed_fixture_is_two_prefix_then_ip16() {
        const SEED: &[u8] = include_bytes!("../tests/fixtures/asmap_two_prefix_plus_ip.bin");
        assert!(SEED.len() > 16, "asmap then leftover ip16");
        let (asmap, ip) = SEED.split_at(SEED.len() - 16);
        assert_eq!(asmap, TWO_PREFIX_ASMAP);
        let m = AsMap::from_bytes(asmap.to_vec()).expect("two-prefix");
        let mut ip16 = [0u8; 16];
        ip16.copy_from_slice(ip);
        assert_eq!(m.interpret_ip16(&ip16), 1);
        assert_eq!(ip16, ip16_for_lookup(IpAddr::V4(Ipv4Addr::new(1, 2, 0, 0))));
    }

    #[test]
    fn sanity_check_bits_rejects_jump_at_zero_bits_and_double_default() {
        assert!(!sanity_check_bits(&[], 128));
        let mut bits = instr_jump(17);
        bits.extend(instr_return(1));
        assert!(!sanity_check_bits(&pack_le(&bits), 0));
        let mut bits = instr_type(DEFAULT);
        bits.extend(encode_bits(1, 1, &ASN_BIT_SIZES));
        bits.extend(instr_type(DEFAULT));
        bits.extend(encode_bits(1, 1, &ASN_BIT_SIZES));
        bits.extend(instr_return(1));
        assert!(AsMap::from_bytes(pack_le(&bits)).is_none());
    }

    fn load(bits: &[bool]) -> Option<AsMap> {
        AsMap::from_bytes(pack_le(bits))
    }

    fn default_then_match_return() -> Vec<bool> {
        let mut bits = instr_type(DEFAULT);
        bits.extend(encode_bits(1, 1, &ASN_BIT_SIZES));
        bits.extend(instr_match(&[false]));
        bits.extend(instr_return(2));
        bits
    }

    #[test]
    fn default_match_return_uses_the_default_only_on_a_miss() {
        let bits = default_then_match_return();
        let m = load(&bits).expect("one default, one match, one return");
        assert_eq!(m.interpret_ip16(&[0u8; 16]), 2, "leading zero matches");
        let mut hi = [0u8; 16];
        hi[0] = 0x80;
        assert_eq!(
            m.interpret_ip16(&hi),
            1,
            "leading one takes the default ASN"
        );
    }

    #[test]
    fn a_second_default_is_rejected() {
        let mut bits = instr_type(DEFAULT);
        bits.extend(encode_bits(1, 1, &ASN_BIT_SIZES));
        bits.extend(default_then_match_return());
        assert!(load(&bits).is_none(), "two defaults in a row");
    }

    #[test]
    fn trailing_one_bit_after_return_is_rejected() {
        let mut bits = instr_return(1);
        bits.push(true);
        assert!(load(&bits).is_none(), "padding bit must be zero");
    }

    #[test]
    fn more_than_seven_trailing_zeros_are_rejected() {
        let mut bits = instr_return(1);
        bits.extend([false; 8]);
        assert!(load(&bits).is_none(), "more than 7 unused bits");
    }

    fn match8(bit: bool) -> Vec<bool> {
        instr_match(&[bit; 8])
    }

    #[test]
    fn a_one_bit_match_keeps_its_width() {
        let mut bits = instr_match(&[false]);
        bits.extend(instr_return(4));
        let m = load(&bits).expect("single short match");
        assert_eq!(m.interpret_ip16(&[0u8; 16]), 4);
    }

    #[test]
    fn two_short_matches_in_a_row_are_rejected() {
        let mut bits = instr_match(&[false]);
        bits.extend(instr_match(&[false]));
        bits.extend(instr_return(1));
        assert!(load(&bits).is_none(), "second short match");
    }

    #[test]
    fn an_eight_bit_match_may_follow_a_short_one() {
        let mut bits = instr_match(&[false]);
        bits.extend(match8(false));
        bits.extend(instr_return(1));
        assert!(load(&bits).is_some(), "match of 8 closes the short run");
    }

    #[test]
    fn a_match_wider_than_the_bits_left_is_rejected() {
        let mut bits = Vec::new();
        for _ in 0..16 {
            bits.extend(match8(false));
        }
        bits.extend(instr_match(&[false]));
        bits.extend(instr_return(1));
        assert!(load(&bits).is_none(), "no IP bits left");
    }

    #[test]
    fn a_match_that_uses_the_last_ip_bits_still_returns() {
        let mut bits = Vec::new();
        for _ in 0..15 {
            bits.extend(match8(false));
        }
        bits.extend(match8(false));
        bits.extend(instr_return(6));
        let m = load(&bits).expect("128 IP bits exactly");
        assert_eq!(m.interpret_ip16(&[0u8; 16]), 6);
    }

    #[test]
    fn a_jump_past_the_file_is_rejected() {
        let mut bits = instr_jump(5_000);
        bits.extend(instr_return(1));
        assert!(
            load(&bits).is_none(),
            "a jump that cannot land on a return is not an asmap"
        );
    }

    #[test]
    fn jump_leaves_too_few_bits_for_the_following_match() {
        let ret = instr_return(1);
        let mut bits = Vec::new();
        for _ in 0..15 {
            bits.extend(match8(false));
        }
        bits.extend(instr_jump(ret.len() as u32));
        bits.extend(ret);
        bits.extend(match8(false));
        bits.extend(instr_return(2));
        assert!(
            load(&bits).is_none(),
            "jump spends a bit the next match still needs"
        );
    }
}
