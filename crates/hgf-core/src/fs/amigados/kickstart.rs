// SPDX-License-Identifier: GPL-3.0-or-later

//! Amiga 1000 Kickstart disks (what WriteKickDisk does on the Amiga).
//!
//! The A1000 has no Kickstart in ROM: its boot ROM loads a 256K Kickstart from a disk into
//! its write-protected "WCS" RAM. The disk is a plain AmigaDOS-sector (MFM) disk laid out as:
//!
//! * block 0: `KICK` followed by zero bytes;
//! * blocks 1-512: the raw 256K ROM image;
//! * blocks 513 and up: unused.
//!
//! So only cylinders 0-23 need writing. 512K Kickstarts (2.0 and later) do not fit the A1000's
//! 256K WCS and are refused.

use super::{image_size, BSIZE};
use crate::fs::bytes::be32;

/// Size of an A1000 Kickstart.
pub const ROM_256K: usize = 256 * 1024;
/// Blocks used on the disk: the `KICK` header plus the ROM.
pub const KICK_BLOCKS: usize = 1 + ROM_256K / BSIZE;
/// Cylinders to write (blocks 0-512 end on cylinder 23).
pub const KICK_CYLINDERS: u16 = 24;

/// Header of the encrypted ROM files of Cloanto's Amiga Forever.
const CLOANTO_HEADER: &[u8] = b"AMIROMTYPE1";

/// A Kickstart ROM ready to be written on an A1000 Kickstart disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KickRom {
    /// The 256K image, big-endian as the 68000 reads it.
    pub data: Vec<u8>,
    /// exec version and revision from the ROM header (e.g. 34.5 for Kickstart 1.3).
    pub version: u16,
    pub revision: u16,
    /// ROM checksum (sum with carry of all longs = 0xFFFFFFFF).
    pub checksum_ok: bool,
    /// What was done to the file to get the image (decrypted, byte-swapped, halved…).
    pub notes: Vec<String>,
}

impl KickRom {
    /// Marketing name of the release (`1.3`), when known.
    pub fn release(&self) -> &'static str {
        match self.version {
            27..=30 => "1.0",
            31 | 32 => "1.1",
            33 => "1.2",
            34 => "1.3",
            _ => "?",
        }
    }

    /// `Kickstart 1.3 (34.5)`.
    pub fn describe(&self) -> String {
        format!("Kickstart {} ({}.{})", self.release(), self.version, self.revision)
    }
}

/// Sum of the ROM longs with end-around carry; a valid Kickstart gives `0xFFFF_FFFF`.
pub fn rom_sum(rom: &[u8]) -> u32 {
    let mut s = 0u32;
    for c in rom.chunks_exact(4) {
        let (n, carry) = s.overflowing_add(be32(c, 0));
        s = n.wrapping_add(carry as u32);
    }
    s
}

/// Turn the content of a ROM file into a 256K A1000 Kickstart.
///
/// Accepted: plain 256K images (GrabKick, transrom, emulator ROM files), 512K files holding the
/// same 256K twice, byte-swapped EPROM dumps, and Amiga Forever encrypted files when their
/// `rom.key` is given.
pub fn load_rom(file: &[u8], key: Option<&[u8]>) -> Result<KickRom, String> {
    let mut notes = Vec::new();
    let mut data = if let Some(rest) = file.strip_prefix(CLOANTO_HEADER) {
        let key = key.filter(|k| !k.is_empty()).ok_or(
            "This ROM file is encrypted (Amiga Forever). Put its rom.key file next to it, \
             in the same folder.",
        )?;
        notes.push("decrypted with rom.key".to_string());
        rest.iter().zip(key.iter().cycle()).map(|(b, k)| b ^ k).collect::<Vec<u8>>()
    } else {
        file.to_vec()
    };
    match data.len() {
        ROM_256K => {}
        n if n == 2 * ROM_256K => {
            if data[..ROM_256K] == data[ROM_256K..] {
                data.truncate(ROM_256K);
                notes.push("512K file holding the same 256K ROM twice: one copy used".into());
            } else {
                return Err("This is a 512K Kickstart (2.0 or later). The Amiga 1000 can only load \
                            a 256K Kickstart (1.0 to 1.3)."
                    .into());
            }
        }
        n => return Err(format!("A Kickstart ROM image is 262144 bytes (256K); this file is {n} bytes.")),
    }
    // ROM header: $1111 (256K ROM) then a JMP ($4EF9). Byte-swapped dumps read $F94E.
    if data[2..4] == [0xF9, 0x4E] {
        for pair in data.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
        notes.push("byte-swapped dump: bytes put back in 68000 order".into());
    }
    if data[..4] != [0x11, 0x11, 0x4E, 0xF9] {
        if data[..2] == [0x11, 0x14] {
            return Err("This is a 512K Kickstart (2.0 or later). The Amiga 1000 can only load \
                        a 256K Kickstart (1.0 to 1.3)."
                .into());
        }
        return Err("This file is not a Kickstart ROM image (no Kickstart header).".into());
    }
    let version = u16::from_be_bytes([data[12], data[13]]);
    let revision = u16::from_be_bytes([data[14], data[15]]);
    let checksum_ok = rom_sum(&data) == 0xFFFF_FFFF;
    Ok(KickRom { data, version, revision, checksum_ok, notes })
}

/// The DD disk image (880K ADF) of an A1000 Kickstart disk.
pub fn kick_disk_image(rom: &KickRom) -> Vec<u8> {
    let mut img = vec![0u8; image_size(false)];
    img[..4].copy_from_slice(b"KICK");
    img[BSIZE..BSIZE + ROM_256K].copy_from_slice(&rom.data);
    img
}

/// Whether an image is an A1000 Kickstart disk.
pub fn is_kick_disk(img: &[u8]) -> bool {
    img.len() >= KICK_BLOCKS * BSIZE && img.starts_with(b"KICK")
}

/// The Kickstart held by an A1000 Kickstart disk image.
pub fn rom_from_kick_disk(img: &[u8]) -> Result<KickRom, String> {
    if !is_kick_disk(img) {
        return Err("Not an Amiga 1000 Kickstart disk (no KICK header).".into());
    }
    load_rom(&img[BSIZE..BSIZE + ROM_256K], None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::bytes::put_be32;

    /// A synthetic 256K Kickstart 34.5 with a valid checksum.
    fn rom() -> Vec<u8> {
        let mut r = vec![0u8; ROM_256K];
        r[..8].copy_from_slice(&[0x11, 0x11, 0x4E, 0xF9, 0x00, 0xFC, 0x00, 0xD2]);
        r[12..16].copy_from_slice(&[0x00, 34, 0x00, 5]);
        for (i, b) in r.iter_mut().enumerate().skip(16).take(1000) {
            *b = (i * 7) as u8;
        }
        // checksum long near the end, as in real ROMs
        let off = ROM_256K - 24;
        put_be32(&mut r, off, 0);
        let s = rom_sum(&r);
        put_be32(&mut r, off, !s);
        r
    }

    #[test]
    fn plain_rom() {
        let k = load_rom(&rom(), None).unwrap();
        assert!(k.checksum_ok);
        assert_eq!((k.version, k.revision), (34, 5));
        assert_eq!(k.describe(), "Kickstart 1.3 (34.5)");
        assert!(k.notes.is_empty());
    }

    #[test]
    fn disk_layout() {
        let k = load_rom(&rom(), None).unwrap();
        let img = kick_disk_image(&k);
        assert_eq!(img.len(), 901_120);
        assert_eq!(&img[..4], b"KICK");
        assert!(img[4..512].iter().all(|&b| b == 0));
        assert_eq!(&img[512..512 + ROM_256K], &k.data[..]);
        assert!(img[KICK_BLOCKS * 512..].iter().all(|&b| b == 0));
        // the last used block (512) is on cylinder 23
        assert_eq!(super::super::cylinder_of(KICK_BLOCKS - 1, false), KICK_CYLINDERS - 1);
        assert!(is_kick_disk(&img));
        assert_eq!(rom_from_kick_disk(&img).unwrap(), k);
    }

    #[test]
    fn swapped_doubled_and_encrypted_files() {
        let r = rom();
        let mut swapped = r.clone();
        for p in swapped.chunks_exact_mut(2) {
            p.swap(0, 1);
        }
        assert_eq!(load_rom(&swapped, None).unwrap().data, r);

        let doubled = [r.clone(), r.clone()].concat();
        assert_eq!(load_rom(&doubled, None).unwrap().data, r);

        let key: Vec<u8> = (0..1024u32).map(|i| (i * 31 + 7) as u8).collect();
        let mut enc = CLOANTO_HEADER.to_vec();
        enc.extend(r.iter().zip(key.iter().cycle()).map(|(b, k)| b ^ k));
        assert!(load_rom(&enc, None).unwrap_err().contains("rom.key"));
        let k = load_rom(&enc, Some(&key)).unwrap();
        assert_eq!(k.data, r);
        assert!(k.checksum_ok);
    }

    #[test]
    fn refuses_what_an_a1000_cannot_load() {
        let mut big = vec![0u8; 2 * ROM_256K];
        big[..4].copy_from_slice(&[0x11, 0x14, 0x4E, 0xF9]);
        big[ROM_256K] = 1;
        assert!(load_rom(&big, None).unwrap_err().contains("512K"));
        assert!(load_rom(&[0u8; 1000], None).unwrap_err().contains("262144"));
        assert!(load_rom(&vec![0u8; ROM_256K], None).unwrap_err().contains("not a Kickstart"));
        let mut bad = rom();
        bad[100] ^= 0xff;
        assert!(!load_rom(&bad, None).unwrap().checksum_ok);
    }
}
