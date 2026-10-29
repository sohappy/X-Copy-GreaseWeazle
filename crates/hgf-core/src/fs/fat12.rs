// SPDX-License-Identifier: GPL-3.0-or-later

//! Blank FAT12 disk images: PC (DOS/Windows), Atari ST (TOS) and MSX (MSX-DOS) floppies.
//!
//! The equivalent of DOS `FORMAT`, `mformat` or the TOS desktop "Format…": a boot sector with
//! the BIOS parameter block, two empty FATs and an empty root directory holding the volume label.
//! Checked with fsck.fat, mtools and pyfatfs in the tests.

use crate::fs::bytes::{now_secs, put_le16 as put16, put_le32 as put32};

const BPS: usize = 512;

/// Machine family of a FAT12 disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum System {
    #[default]
    Pc,
    AtariSt,
    Msx,
}

impl System {
    pub const ALL: [System; 3] = [System::Pc, System::AtariSt, System::Msx];

    pub fn label(self) -> &'static str {
        match self {
            System::Pc => "PC (DOS / Windows)",
            System::AtariSt => "Atari ST (TOS)",
            System::Msx => "MSX (MSX-DOS)",
        }
    }
}

/// A standard FAT12 floppy layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Pc360,
    Pc720,
    Pc1200,
    Pc1440,
    Pc2880,
    St360,
    St720,
    Msx360,
    Msx720,
}

/// Geometry and BIOS parameter block of one layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub label: &'static str,
    pub note: &'static str,
    /// gw `--format`.
    pub gw_format: &'static str,
    pub extension: &'static str,
    pub cyls: u16,
    pub heads: u16,
    pub spt: u16,
    pub spc: u8,
    pub root_entries: u16,
    pub media: u8,
    /// Sectors per FAT.
    pub spf: u16,
    pub hd: bool,
}

impl Layout {
    pub fn sectors(&self) -> usize {
        self.cyls as usize * self.heads as usize * self.spt as usize
    }

    pub fn bytes(&self) -> usize {
        self.sectors() * BPS
    }

    /// Sectors used by the boot sector, the two FATs and the root directory.
    fn system_sectors(&self) -> usize {
        1 + 2 * self.spf as usize + (self.root_entries as usize * 32).div_ceil(BPS)
    }

    /// Free space of the blank disk, in bytes.
    pub fn free_bytes(&self) -> usize {
        let clusters = (self.sectors() - self.system_sectors()) / self.spc as usize;
        clusters * self.spc as usize * BPS
    }
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Pc360,
        Kind::Pc720,
        Kind::Pc1200,
        Kind::Pc1440,
        Kind::Pc2880,
        Kind::St360,
        Kind::St720,
        Kind::Msx360,
        Kind::Msx720,
    ];

    pub fn system(self) -> System {
        match self {
            Kind::Pc360 | Kind::Pc720 | Kind::Pc1200 | Kind::Pc1440 | Kind::Pc2880 => System::Pc,
            Kind::St360 | Kind::St720 => System::AtariSt,
            Kind::Msx360 | Kind::Msx720 => System::Msx,
        }
    }

    pub fn of(system: System) -> impl Iterator<Item = Kind> {
        Kind::ALL.into_iter().filter(move |k| k.system() == system)
    }

    /// The usual choice for a system.
    pub fn default_for(system: System) -> Kind {
        match system {
            System::Pc => Kind::Pc1440,
            System::AtariSt => Kind::St720,
            System::Msx => Kind::Msx720,
        }
    }

    /// Standard DOS / TOS / MSX-DOS values (the same as `FORMAT`, `mformat` and TOS produce).
    pub fn layout(self) -> Layout {
        let l = |label, note, gw_format, extension, cyls, heads, spt, spc, root_entries, media, spf, hd| {
            Layout { label, note, gw_format, extension, cyls, heads, spt, spc, root_entries, media, spf, hd }
        };
        match self {
            Kind::Pc360 => l(
                "360K (5.25\" DD)",
                "40-track 5.25\" DD disk (PC/XT). Needs a 5.25\" drive.",
                "ibm.360",
                "img",
                40,
                2,
                9,
                2,
                112,
                0xfd,
                2,
                false,
            ),
            Kind::Pc720 => l(
                "720K (3.5\" DD)",
                "3.5\" DD disk (one square hole).",
                "ibm.720",
                "img",
                80,
                2,
                9,
                2,
                112,
                0xf9,
                3,
                false,
            ),
            Kind::Pc1200 => l(
                "1.2M (5.25\" HD)",
                "5.25\" HD disk in a 5.25\" HD drive (360 rpm).",
                "ibm.1200",
                "img",
                80,
                2,
                15,
                1,
                224,
                0xf9,
                7,
                true,
            ),
            Kind::Pc1440 => l(
                "1.44M (3.5\" HD)",
                "3.5\" HD disk (two square holes). The most common PC floppy.",
                "ibm.1440",
                "img",
                80,
                2,
                18,
                1,
                224,
                0xf0,
                9,
                true,
            ),
            Kind::Pc2880 => l(
                "2.88M (3.5\" ED)",
                "3.5\" ED disk in an ED drive only (IBM PS/2, some workstations).",
                "ibm.2880",
                "img",
                80,
                2,
                36,
                2,
                240,
                0xf0,
                9,
                true,
            ),
            Kind::St360 => l(
                "360K single-sided",
                "Single-sided TOS disk (SF354 drive, early 520ST).",
                "atarist.360",
                "st",
                80,
                1,
                9,
                2,
                112,
                0xf8,
                5,
                false,
            ),
            Kind::St720 => l(
                "720K double-sided",
                "Double-sided TOS disk: every ST, STE, TT and Falcon. Also readable on a PC.",
                "atarist.720",
                "st",
                80,
                2,
                9,
                2,
                112,
                0xf9,
                5,
                false,
            ),
            Kind::Msx360 => l(
                "360K single-sided (1DD)",
                "Single-sided MSX-DOS disk (early MSX1 drives).",
                "msx.1dd",
                "img",
                80,
                1,
                9,
                2,
                112,
                0xf8,
                2,
                false,
            ),
            Kind::Msx720 => l(
                "720K double-sided (2DD)",
                "Double-sided MSX-DOS disk: MSX2 and most MSX1 drives. Also readable on a PC.",
                "msx.2dd",
                "img",
                80,
                2,
                9,
                2,
                112,
                0xf9,
                3,
                false,
            ),
        }
    }
}

/// Options of the blank FAT disk.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FatOpts {
    pub system: System,
    pub kind: Kind,
    /// Volume label; empty = no label.
    pub label: String,
}

impl Default for FatOpts {
    fn default() -> Self {
        Self { system: System::Pc, kind: Kind::Pc1440, label: String::new() }
    }
}

/// Characters allowed in a FAT volume label (stored upper-case).
pub fn validate_label(label: &str) -> Result<(), String> {
    if label.chars().count() > 11 {
        return Err("At most 11 characters".into());
    }
    if let Some(c) = label.chars().find(|c| !(c.is_ascii_alphanumeric() || " !#$%&'()-@^_`{}~".contains(*c)))
    {
        return Err(format!("Character '{c}' is not allowed in a FAT label"));
    }
    if label.starts_with(' ') {
        return Err("The label cannot start with a space".into());
    }
    Ok(())
}

/// Minimal x86 boot code: prints the message, waits for a key and reboots (`int 19h`).
/// Same behaviour as the boot sector of a disk formatted without system files.
fn pc_boot_code(sector: &mut [u8]) {
    // At 0x3E: cli; xor ax,ax; mov ds,ax; mov ss,ax; mov sp,7C00h; sti; mov si,7C60h
    // next: lodsb; or al,al; jz key; mov ah,0Eh; mov bx,7; int 10h; jmp next
    // key:  xor ax,ax; int 16h; int 19h
    const CODE: [u8; 34] = [
        0xfa, 0x31, 0xc0, 0x8e, 0xd8, 0x8e, 0xd0, 0xbc, 0x00, 0x7c, 0xfb, 0xbe, 0x60, 0x7c, 0xac, 0x08, 0xc0,
        0x74, 0x09, 0xb4, 0x0e, 0xbb, 0x07, 0x00, 0xcd, 0x10, 0xeb, 0xf2, 0x31, 0xc0, 0xcd, 0x16, 0xcd, 0x19,
    ];
    sector[0x3e..0x3e + CODE.len()].copy_from_slice(&CODE);
    let msg = b"\r\nNon-system disk. Press any key to restart.\r\n\0";
    sector[0x60..0x60 + msg.len()].copy_from_slice(msg);
}

/// Upper-cased label padded to 11 bytes ("NO NAME" when empty, as DOS does in the boot sector).
fn label_bytes(label: &str, empty: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    let text = if label.trim().is_empty() { empty.to_string() } else { label.to_ascii_uppercase() };
    for (o, c) in out.iter_mut().zip(text.bytes()) {
        *o = c;
    }
    out
}

/// UTC calendar date and time of `secs` since the Unix epoch: (year, month, day, h, min, s).
pub fn utc(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as u32, day as u32, (rem / 3600) as u32, ((rem / 60) % 60) as u32, (rem % 60) as u32)
}

/// FAT date and time of `secs` since the Unix epoch (UTC).
fn fat_datetime(secs: u64) -> (u16, u16) {
    let (year, month, day, h, m, s) = utc(secs);
    let year = year.clamp(1980, 2107);
    let date = (((year - 1980) as u16) << 9) | ((month as u16) << 5) | day as u16;
    let time = ((h as u16) << 11) | ((m as u16) << 5) | (s / 2) as u16;
    (date, time)
}

/// Build the blank image (`now`: seconds since the Unix epoch, for the serial number and label date).
pub fn blank_image_at(o: &FatOpts, now: u64) -> Result<Vec<u8>, String> {
    validate_label(&o.label)?;
    let l = o.kind.layout();
    let mut img = vec![0u8; l.bytes()];
    let serial = (now as u32).wrapping_mul(0x9e37_79b9) ^ 0x5a5a_1234;
    let boot = &mut img[..BPS];

    match o.kind.system() {
        System::Pc | System::Msx => {
            boot[..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
            boot[3..11].copy_from_slice(b"MSDOS5.0");
        }
        System::AtariSt => {
            // TOS reads the BPB and ignores the first bytes; an x86 jump keeps the disk readable
            // on a PC. Bytes 8-10 are the serial number TOS uses to detect a disk change.
            boot[..3].copy_from_slice(&[0xe9, 0x00, 0x00]);
            boot[3..8].copy_from_slice(b"HGF  ");
            boot[8..11].copy_from_slice(&serial.to_le_bytes()[..3]);
        }
    }
    put16(boot, 11, BPS as u16);
    boot[13] = l.spc;
    put16(boot, 14, 1); // reserved sectors
    boot[16] = 2; // FATs
    put16(boot, 17, l.root_entries);
    put16(boot, 19, l.sectors() as u16);
    boot[21] = l.media;
    put16(boot, 22, l.spf);
    put16(boot, 24, l.spt);
    put16(boot, 26, l.heads);
    // Hidden sectors (28) and 32-bit total (32) stay 0.
    // Extended BPB (DOS 4+). TOS ignores these bytes on a non-bootable disk, and PCs want them.
    boot[36] = 0x00; // BIOS drive number (floppy)
    boot[38] = 0x29; // extended boot signature
    put32(boot, 39, serial);
    boot[43..54].copy_from_slice(&label_bytes(&o.label, "NO NAME"));
    boot[54..62].copy_from_slice(b"FAT12   ");
    pc_boot_code(boot);
    boot[510] = 0x55;
    boot[511] = 0xaa;
    if o.kind.system() == System::AtariSt {
        // Not executable on the ST: the boot sector checksum must not be 0x1234.
        let sum = boot.chunks_exact(2).fold(0u16, |s, w| s.wrapping_add(u16::from_be_bytes([w[0], w[1]])));
        if sum == 0x1234 {
            boot[509] ^= 1;
        }
    }

    // Both FATs: media descriptor, then the end-of-chain marker of reserved cluster 1.
    for f in 0..2 {
        let off = BPS * (1 + f * l.spf as usize);
        img[off..off + 3].copy_from_slice(&[l.media, 0xff, 0xff]);
    }

    // Root directory: the volume label entry.
    if !o.label.trim().is_empty() {
        let off = BPS * (1 + 2 * l.spf as usize);
        let e = &mut img[off..off + 32];
        e[..11].copy_from_slice(&label_bytes(&o.label, ""));
        e[11] = 0x08; // volume label attribute
        let (date, time) = fat_datetime(now);
        put16(e, 22, time);
        put16(e, 24, date);
    }
    Ok(img)
}

pub fn blank_image(o: &FatOpts) -> Result<Vec<u8>, String> {
    let now = now_secs();
    blank_image_at(o, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn opts(kind: Kind, label: &str) -> FatOpts {
        FatOpts { system: kind.system(), kind, label: label.into() }
    }

    #[test]
    fn sizes_match_the_gw_formats() {
        let sizes = [
            (Kind::Pc360, 368_640),
            (Kind::Pc720, 737_280),
            (Kind::Pc1200, 1_228_800),
            (Kind::Pc1440, 1_474_560),
            (Kind::Pc2880, 2_949_120),
            (Kind::St360, 368_640),
            (Kind::St720, 737_280),
            (Kind::Msx360, 368_640),
            (Kind::Msx720, 737_280),
        ];
        for (k, size) in sizes {
            assert_eq!(blank_image_at(&opts(k, "X"), 0).unwrap().len(), size, "{k:?}");
        }
    }

    #[test]
    fn each_fat_is_large_enough_for_its_clusters() {
        for k in Kind::ALL {
            let l = k.layout();
            let clusters = l.free_bytes() / (l.spc as usize * BPS) + 2;
            assert!(l.spf as usize * BPS * 2 >= clusters * 3, "{k:?}: FAT too small");
            // FAT12 limit.
            assert!(clusters < 4085, "{k:?}");
        }
        assert_eq!(Kind::Pc1440.layout().free_bytes(), 1_457_664); // what DOS reports
        assert_eq!(Kind::Pc720.layout().free_bytes(), 730_112);
    }

    #[test]
    fn labels() {
        assert!(validate_label("GAMES_1").is_ok());
        assert!(validate_label("").is_ok());
        assert!(validate_label("TOO LONG LABEL").is_err());
        assert!(validate_label("a/b").is_err());
        let img = blank_image_at(&opts(Kind::Pc1440, "data disk"), 0).unwrap();
        assert_eq!(&img[43..54], b"DATA DISK  ");
        let root = 512 * 19;
        assert_eq!(&img[root..root + 11], b"DATA DISK  ");
        assert_eq!(img[root + 11], 0x08);
    }

    #[test]
    fn atari_boot_sector_is_not_executable() {
        for t in 0..64 {
            let img = blank_image_at(&opts(Kind::St720, ""), t).unwrap();
            let sum = img[..512]
                .chunks_exact(2)
                .fold(0u16, |s, w| s.wrapping_add(u16::from_be_bytes([w[0], w[1]])));
            assert_ne!(sum, 0x1234);
            assert_eq!(u16::from_le_bytes([img[22], img[23]]), 5, "TOS uses 5 sectors per FAT");
        }
    }

    #[test]
    fn fat_dates() {
        // 2026-10-17 18:17:30 UTC
        let (d, t) = fat_datetime(1_792_261_050);
        assert_eq!((d >> 9) + 1980, 2026);
        assert_eq!((d >> 5) & 15, 10);
        assert_eq!(d & 31, 17);
        assert_eq!((t >> 11, (t >> 5) & 63, (t & 31) * 2), (18, 17, 30));
    }

    /// Cross-check with independent FAT implementations when they are installed
    /// (fsck.fat from dosfstools, mdir from mtools).
    #[test]
    fn external_tools_accept_the_images() {
        let dir = std::env::temp_dir().join(format!("hgf-fat12-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for k in Kind::ALL {
            let p = dir.join(format!("{k:?}.img"));
            std::fs::write(&p, blank_image_at(&opts(k, "HGF TEST"), 1_792_261_050).unwrap()).unwrap();
            if let Ok(out) = Command::new("fsck.fat").arg("-n").arg(&p).output() {
                let text =
                    String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
                assert!(out.status.success(), "fsck.fat {k:?}: {text}");
                assert!(!text.contains("differ"), "fsck.fat {k:?}: {text}");
            }
            if let Ok(out) = Command::new("mdir").arg("-i").arg(&p).arg("::").output() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                assert!(out.status.success(), "mdir {k:?}: {}", String::from_utf8_lossy(&out.stderr));
                assert!(text.contains("HGF TEST"), "mdir {k:?}: {text}");
                assert!(text.contains("No files"), "mdir {k:?}: {text}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
