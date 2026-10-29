// SPDX-License-Identifier: GPL-3.0-or-later

//! AmigaDOS (OFS/FFS) floppies: block layout and checksums, blank disks ([`format`]),
//! a read-only explorer ([`explore`]) and the analyses used by copiers ([`analyse`]:
//! allocation bitmap, volume name, boot block) and Amiga 1000 Kickstart disks ([`kickstart`]).

mod analyse;
mod explore;
mod format;
pub mod kickstart;

pub use analyse::*;
pub use explore::*;
pub use format::*;

use crate::fs::bytes::{be32, put_be32};

/// Bytes per block.
pub const BSIZE: usize = 512;
/// Blocks of a DD (880K) and HD (1.76M) disk.
pub const DD_BLOCKS: usize = 1760;
pub const HD_BLOCKS: usize = 3520;

pub fn total_blocks(hd: bool) -> usize {
    if hd {
        HD_BLOCKS
    } else {
        DD_BLOCKS
    }
}

pub fn sectors_per_track(hd: bool) -> usize {
    if hd {
        22
    } else {
        11
    }
}

/// Size in bytes of an ADF.
pub fn image_size(hd: bool) -> usize {
    total_blocks(hd) * BSIZE
}

/// The root block sits in the middle of the disk.
pub fn root_index(hd: bool) -> usize {
    total_blocks(hd) / 2
}

/// Cylinder holding a block (two heads per cylinder).
pub fn cylinder_of(blk: usize, hd: bool) -> u16 {
    (blk / sectors_per_track(hd) / 2) as u16
}

pub(crate) fn block(data: &[u8], n: usize) -> Option<&[u8]> {
    data.get(n * BSIZE..(n + 1) * BSIZE)
}

/// Header blocks (root, bitmap, file and directory headers): the sum of all longs is zero.
pub fn header_checksum_ok(block: &[u8]) -> bool {
    block.chunks_exact(4).map(|c| be32(c, 0)).fold(0u32, u32::wrapping_add) == 0
}

/// Store the checksum long at `off` so that [`header_checksum_ok`] holds.
pub fn set_header_checksum(block: &mut [u8], off: usize) {
    put_be32(block, off, 0);
    let sum = block.chunks_exact(4).map(|c| be32(c, 0)).fold(0u32, u32::wrapping_add);
    put_be32(block, off, 0u32.wrapping_sub(sum));
}

/// Boot block (first 1024 bytes) sum with end-around carry; Kickstart runs the boot code
/// only when it equals `0xffff_ffff`.
pub fn boot_sum(boot: &[u8]) -> u32 {
    let mut s = 0u32;
    for c in boot.chunks_exact(4) {
        let (n, carry) = s.overflowing_add(be32(c, 0));
        s = n.wrapping_add(carry as u32);
    }
    s
}

pub fn boot_checksum_ok(boot: &[u8]) -> bool {
    boot.len() >= 1024 && boot_sum(&boot[..1024]) == 0xffff_ffff
}

/// Store the boot block checksum (long at offset 4).
pub fn set_boot_checksum(boot: &mut [u8]) {
    put_be32(boot, 4, 0);
    let s = boot_sum(&boot[..1024]);
    put_be32(boot, 4, !s);
}

/// BCPL string (length byte + characters), as used for names in header blocks.
pub(crate) fn bcpl_name(block: &[u8], off: usize) -> Option<String> {
    let n = *block.get(off)? as usize;
    if n == 0 || n > 30 || off + 1 + n > block.len() {
        return None;
    }
    Some(block[off + 1..off + 1 + n].iter().map(|&c| c as char).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_round_trip() {
        let mut b = vec![7u8; 512];
        set_header_checksum(&mut b, 20);
        assert!(header_checksum_ok(&b));
        let mut boot = vec![0u8; 1024];
        boot[..4].copy_from_slice(b"DOS\0");
        boot[12] = 0x4e;
        set_boot_checksum(&mut boot);
        assert!(boot_checksum_ok(&boot));
    }

    #[test]
    fn geometry() {
        assert_eq!(root_index(false), 880);
        assert_eq!(cylinder_of(880, false), 40);
        assert_eq!(cylinder_of(1760, true), 40);
        assert_eq!(image_size(true), 1_802_240);
    }
}
