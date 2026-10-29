// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte-level helpers shared by every filesystem module (big-endian Amiga structures,
//! little-endian FAT structures) and the current time for date stamps.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn be32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

pub fn put_be32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

pub fn put_le16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

pub fn put_le32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Seconds since the Unix epoch (0 if the clock is before 1970).
pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endianness() {
        let mut b = [0u8; 8];
        put_be32(&mut b, 0, 0x0102_0304);
        put_le16(&mut b, 4, 0x0506);
        assert_eq!(b[..6], [1, 2, 3, 4, 6, 5]);
        assert_eq!(be32(&b, 0), 0x0102_0304);
        put_le32(&mut b, 4, 0x0a0b_0c0d);
        assert_eq!(b[4..], [0x0d, 0x0c, 0x0b, 0x0a]);
    }
}
