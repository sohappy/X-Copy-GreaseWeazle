// SPDX-License-Identifier: GPL-3.0-or-later

//! Creation of blank AmigaDOS disk images (the equivalent of the Workbench "Format" command).
//!
//! Layout (DD 880K / HD 1.76M): boot block in blocks 0-1, root block in the middle of the disk
//! (880 / 1760), one bitmap block right after it. Checked against `xdftool … format`
//! (amitools) in the tests.

use super::{set_header_checksum, BSIZE, DD_BLOCKS, HD_BLOCKS};
use crate::fs::bytes::{now_secs, put_be32 as put32};

/// Filesystems that a freshly formatted disk can carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Filesystem {
    /// DOS0: readable by every Amiga, Kickstart 1.x included.
    #[default]
    Ofs,
    /// DOS1: Kickstart 2.0 and later.
    Ffs,
}

impl Filesystem {
    pub fn label(self) -> &'static str {
        match self {
            Filesystem::Ofs => "OFS (all Amigas, Kickstart 1.x)",
            Filesystem::Ffs => "FFS (Kickstart 2.0+)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FormatOpts {
    pub name: String,
    pub filesystem: Filesystem,
    /// International mode (DOS2/DOS3): correct case-folding of accented names.
    pub international: bool,
    pub hd: bool,
}

impl Default for FormatOpts {
    fn default() -> Self {
        Self { name: "Empty".into(), filesystem: Filesystem::Ofs, international: false, hd: false }
    }
}

impl FormatOpts {
    /// DOS type byte: `DOS\0`..`DOS\3`.
    pub fn dos_type(&self) -> u8 {
        (self.filesystem == Filesystem::Ffs) as u8 | ((self.international as u8) << 1)
    }

    pub fn gw_format(&self) -> &'static str {
        if self.hd {
            "amiga.amigados_hd"
        } else {
            "amiga.amigados"
        }
    }

    pub fn blocks(&self) -> usize {
        if self.hd {
            HD_BLOCKS
        } else {
            DD_BLOCKS
        }
    }
}

/// Check a volume name the way AmigaDOS does.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("The volume name cannot be empty.".into());
    }
    if name.len() > 30 {
        return Err("The volume name is limited to 30 characters.".into());
    }
    if name.contains(':') || name.contains('/') {
        return Err("The volume name cannot contain ':' or '/'.".into());
    }
    if !name.chars().all(|c| (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c)) {
        return Err("The volume name can only use Latin-1 characters.".into());
    }
    Ok(())
}

/// AmigaDOS date stamp (days since 1978-01-01, minutes, ticks of 1/50 s) for a Unix time.
pub fn amiga_date(unix_secs: u64) -> (u32, u32, u32) {
    const EPOCH_DIFF: u64 = 2922 * 86_400; // 1970-01-01 → 1978-01-01
    let s = unix_secs.saturating_sub(EPOCH_DIFF);
    ((s / 86_400) as u32, ((s % 86_400) / 60) as u32, ((s % 60) * 50) as u32)
}

/// Build a blank, formatted AmigaDOS image.
pub fn blank_image(opts: &FormatOpts) -> Result<Vec<u8>, String> {
    let now = now_secs();
    blank_image_at(opts, now)
}

pub fn blank_image_at(opts: &FormatOpts, unix_secs: u64) -> Result<Vec<u8>, String> {
    validate_name(&opts.name)?;
    let blocks = opts.blocks();
    let mut img = vec![0u8; blocks * BSIZE];
    let root = blocks / 2;
    let bitmap = root + 1;

    // Boot block: DOS type, (no boot code: not bootable, like Workbench "Format"), root pointer.
    img[0..3].copy_from_slice(b"DOS");
    img[3] = opts.dos_type();
    put32(&mut img, 8, root as u32);

    // Root block.
    let (days, mins, ticks) = amiga_date(unix_secs);
    {
        let r = &mut img[root * BSIZE..(root + 1) * BSIZE];
        put32(r, 0, 2); // T_HEADER
        put32(r, 12, 72); // hash table size for 512-byte blocks
        put32(r, 312, 0xffff_ffff); // bitmap valid
        put32(r, 316, bitmap as u32); // bm_pages[0]
        for off in [420, 472, 484] {
            put32(r, off, days);
            put32(r, off + 4, mins);
            put32(r, off + 8, ticks);
        }
        r[432] = opts.name.len() as u8;
        for (i, c) in opts.name.chars().enumerate() {
            r[433 + i] = c as u32 as u8;
        }
        put32(r, 508, 1); // ST_ROOT
        set_header_checksum(r, 20);
    }

    // Bitmap: one bit per block from block 2, 1 = free. Root and bitmap are in use.
    {
        let b = &mut img[bitmap * BSIZE..(bitmap + 1) * BSIZE];
        for long in b[4..].chunks_exact_mut(4) {
            long.copy_from_slice(&[0xff; 4]);
        }
        for used in [root, bitmap] {
            let i = used - 2;
            let off = 4 + (i / 32) * 4;
            let v = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) & !(1 << (i % 32));
            put32(b, off, v);
        }
        set_header_checksum(b, 0);
    }
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::amigados::inspect_adf;
    use crate::fs::bytes::be32;

    #[test]
    fn matches_xdftool_layout() {
        // Reference: `xdftool ref.adf format "Vierge" ofs` (amitools 0.8). Non-zero blocks are
        // 0, 880 and 881; the only differences allowed are the dates (and their checksum) and
        // amitools' private dostype copy in the root's last-but-three long.
        let img = blank_image_at(&FormatOpts { name: "Vierge".into(), ..Default::default() }, 0).unwrap();
        assert_eq!(img.len(), 901_120);
        assert_eq!(&img[..12], &hex::decode("444f53000000000000000370").unwrap()[..]);
        let nonzero: Vec<usize> =
            (0..DD_BLOCKS).filter(|&i| img[i * 512..(i + 1) * 512].iter().any(|&b| b != 0)).collect();
        assert_eq!(nonzero, vec![0, 880, 881]);
        let bm = &img[881 * 512..882 * 512];
        // Bitmap identical to xdftool's: checksum, then all blocks free except 880 and 881.
        for off in (0..512).step_by(4) {
            let expected = match off {
                0 => "0000c07f",
                112 => "ffff3fff",
                _ => "ffffffff",
            };
            assert_eq!(hex::encode(&bm[off..off + 4]), expected, "bitmap long at {off}");
        }
    }

    #[test]
    fn readable_by_the_inspector() {
        for (fs, intl, t) in [
            (Filesystem::Ofs, false, 0u8),
            (Filesystem::Ffs, false, 1),
            (Filesystem::Ofs, true, 2),
            (Filesystem::Ffs, true, 3),
        ] {
            let o = FormatOpts { name: "Work disk".into(), filesystem: fs, international: intl, hd: false };
            let img = blank_image(&o).unwrap();
            assert_eq!(img[3], t);
            let info = inspect_adf(&img).unwrap();
            assert_eq!(info.volume, "Work disk");
            assert!(info.root_valid && info.root_checksum_ok);
            assert!(info.entries.is_empty());
            let bm = &img[881 * 512..882 * 512];
            let sum = bm.chunks_exact(4).map(|c| be32(c, 0)).fold(0u32, u32::wrapping_add);
            assert_eq!(sum, 0, "bitmap checksum");
        }
    }

    #[test]
    fn hd_layout() {
        let img = blank_image(&FormatOpts { hd: true, ..Default::default() }).unwrap();
        assert_eq!(img.len(), 1_802_240);
        assert_eq!(be32(&img, 8), 1760);
        assert_eq!(be32(&img[1760 * 512..], 316), 1761);
        assert!(inspect_adf(&img).unwrap().root_checksum_ok);
    }

    #[test]
    fn names_and_dates() {
        assert!(validate_name("").is_err());
        assert!(validate_name("a:b").is_err());
        assert!(validate_name(&"x".repeat(31)).is_err());
        assert!(validate_name("Disquette été").is_ok());
        // 1978-01-02 00:01:01 UTC
        assert_eq!(amiga_date((2922 + 1) * 86_400 + 61), (1, 1, 50));
    }
}
