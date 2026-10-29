// SPDX-License-Identifier: GPL-3.0-or-later

//! Blank Commodore DOS disk images: 1541 (D64), 1571 (D71) and 1581 (D81).
//!
//! The same layout as `HEADER"NAME,ID"` (or `NEW`) on the drive, and as VICE's
//! `c1541 -format "name,id"`, which the tests compare against byte for byte.

const BLOCK: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Drive {
    /// 5.25" single-sided, 35 tracks, 170K (C64, VIC-20, C16/Plus4).
    #[default]
    D1541,
    /// 5.25" double-sided, 70 tracks, 340K (C128).
    D1571,
    /// 3.5" DD, 80 tracks, 800K.
    D1581,
}

impl Drive {
    pub const ALL: [Drive; 3] = [Drive::D1541, Drive::D1571, Drive::D1581];

    pub fn label(self) -> &'static str {
        match self {
            Drive::D1541 => "1541 (C64) 170K",
            Drive::D1571 => "1571 (C128) 340K",
            Drive::D1581 => "1581 (3.5\") 800K",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Drive::D1541 => "5.25\" disk, one side, 35 tracks, GCR. Read by every 1541, 1570 and 1571.",
            Drive::D1571 => "5.25\" disk, both sides. Needs a 1571 drive (C128); a 1541 reads only side 1.",
            Drive::D1581 => "3.5\" DD disk (one square hole), MFM. Needs a 1581 drive.",
        }
    }

    pub fn gw_format(self) -> &'static str {
        match self {
            Drive::D1541 => "commodore.1541",
            Drive::D1571 => "commodore.1571",
            Drive::D1581 => "commodore.1581",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Drive::D1541 => "d64",
            Drive::D1571 => "d71",
            Drive::D1581 => "d81",
        }
    }

    /// Blocks free on the blank disk (what the directory listing shows).
    pub fn blocks_free(self) -> u32 {
        match self {
            Drive::D1541 => 664,
            Drive::D1571 => 1328,
            Drive::D1581 => 3160,
        }
    }

    /// 5.25" GCR disk (needs a 40-track 5.25" drive or double step).
    pub fn is_525(self) -> bool {
        self != Drive::D1581
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CbmOpts {
    pub drive: Drive,
    pub name: String,
    /// Two-character disk ID.
    pub id: String,
}

impl Default for CbmOpts {
    fn default() -> Self {
        Self { drive: Drive::D1541, name: "EMPTY".into(), id: "01".into() }
    }
}

fn petscii_ok(c: char) -> bool {
    c.is_ascii_alphanumeric() || " !#$%&'()+-./;<>@[]^_".contains(c)
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Enter a disk name".into());
    }
    if name.chars().count() > 16 {
        return Err("At most 16 characters".into());
    }
    if let Some(c) = name.chars().find(|&c| !petscii_ok(c)) {
        return Err(format!("Character '{c}' is not allowed in a disk name"));
    }
    Ok(())
}

pub fn validate_id(id: &str) -> Result<(), String> {
    if id.chars().count() != 2 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("The disk ID is 2 letters or digits".into());
    }
    Ok(())
}

/// ASCII → PETSCII (unshifted: letters upper case), padded with shifted spaces ($A0).
fn petscii(text: &str, len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = text.bytes().map(|b| b.to_ascii_uppercase()).take(len).collect();
    out.resize(len, 0xa0);
    out
}

/// Sectors per track (1-based track numbers) of the 1541/1571 GCR zones.
fn gcr_sectors(track: usize) -> usize {
    let t = (track - 1) % 35 + 1;
    match t {
        1..=17 => 21,
        18..=24 => 19,
        25..=30 => 18,
        _ => 17,
    }
}

/// Byte offset of a 1541/1571 sector (1-based track).
fn gcr_offset(track: usize, sector: usize) -> usize {
    ((1..track).map(gcr_sectors).sum::<usize>() + sector) * BLOCK
}

/// 3-byte free-sector bitmap (bit set = free).
fn bitmap3(sectors: usize, used: &[usize]) -> [u8; 3] {
    let mut m = [0u8; 3];
    for s in (0..sectors).filter(|s| !used.contains(s)) {
        m[s / 8] |= 1 << (s % 8);
    }
    m
}

pub fn blank_image(o: &CbmOpts) -> Result<Vec<u8>, String> {
    validate_name(&o.name)?;
    validate_id(&o.id)?;
    let id = petscii(&o.id, 2);
    Ok(match o.drive {
        Drive::D1541 | Drive::D1571 => gcr_disk(o, &id),
        Drive::D1581 => d81(o, &id),
    })
}

fn gcr_disk(o: &CbmOpts, id: &[u8]) -> Vec<u8> {
    let tracks = if o.drive == Drive::D1571 { 70 } else { 35 };
    let size: usize = (1..=tracks).map(gcr_sectors).sum::<usize>() * BLOCK;
    let mut img = vec![0u8; size];
    let bam_off = gcr_offset(18, 0);
    {
        let bam = &mut img[bam_off..bam_off + BLOCK];
        bam[0] = 18;
        bam[1] = 1;
        bam[2] = b'A';
        bam[3] = if o.drive == Drive::D1571 { 0x80 } else { 0 };
        for t in 1..=35 {
            let n = gcr_sectors(t);
            let used: &[usize] = if t == 18 { &[0, 1] } else { &[] };
            let e = 4 + (t - 1) * 4;
            bam[e] = (n - used.len()) as u8;
            bam[e + 1..e + 4].copy_from_slice(&bitmap3(n, used));
        }
        bam[0x90..0xa0].copy_from_slice(&petscii(&o.name, 16));
        bam[0xa0] = 0xa0;
        bam[0xa1] = 0xa0;
        bam[0xa2..0xa4].copy_from_slice(id);
        bam[0xa4] = 0xa0;
        bam[0xa5] = b'2';
        bam[0xa6] = b'A';
        bam[0xa7..0xab].fill(0xa0);
        if o.drive == Drive::D1571 {
            // Free counts of tracks 36-70; track 53 (side 2 BAM) is reserved entirely.
            for t in 36..=70 {
                bam[0xdd + t - 36] = if t == 53 { 0 } else { gcr_sectors(t) as u8 };
            }
        }
    }
    if o.drive == Drive::D1571 {
        let off = gcr_offset(53, 0);
        for t in 36..=70 {
            let n = gcr_sectors(t);
            let all: Vec<usize> = (0..n).collect();
            let used: &[usize] = if t == 53 { &all } else { &[] };
            let e = off + (t - 36) * 3;
            img[e..e + 3].copy_from_slice(&bitmap3(n, used));
        }
    }
    let dir = gcr_offset(18, 1);
    img[dir + 1] = 0xff;
    img
}

fn d81(o: &CbmOpts, id: &[u8]) -> Vec<u8> {
    const SPT: usize = 40;
    let mut img = vec![0u8; 80 * SPT * BLOCK];
    let t40 = 39 * SPT * BLOCK;
    {
        let h = &mut img[t40..t40 + BLOCK];
        h[0] = 40;
        h[1] = 3;
        h[2] = b'D';
        h[4..0x14].copy_from_slice(&petscii(&o.name, 16));
        h[0x14] = 0xa0;
        h[0x15] = 0xa0;
        h[0x16..0x18].copy_from_slice(id);
        h[0x18] = 0xa0;
        h[0x19] = b'3';
        h[0x1a] = b'D';
        h[0x1b] = 0xa0;
        h[0x1c] = 0xa0;
    }
    for side in 0..2 {
        let off = t40 + (1 + side) * BLOCK;
        let b = &mut img[off..off + BLOCK];
        if side == 0 {
            b[0] = 40;
            b[1] = 2;
        } else {
            b[1] = 0xff;
        }
        b[2] = b'D';
        b[3] = !b'D';
        b[4..6].copy_from_slice(id);
        b[6] = 0xc0; // I/O byte: verify on, check header CRC
        for i in 0..40 {
            let track = side * 40 + i + 1;
            let used: &[usize] = if track == 40 { &[0, 1, 2, 3] } else { &[] };
            let e = 0x10 + i * 6;
            b[e] = (SPT - used.len()) as u8;
            for s in (0..SPT).filter(|s| !used.contains(s)) {
                b[e + 1 + s / 8] |= 1 << (s % 8);
            }
        }
    }
    img[t40 + 3 * BLOCK + 1] = 0xff;
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn opts(drive: Drive) -> CbmOpts {
        CbmOpts { drive, name: "test disk".into(), id: "ab".into() }
    }

    #[test]
    fn sizes_and_free_blocks() {
        assert_eq!(blank_image(&opts(Drive::D1541)).unwrap().len(), 174_848);
        assert_eq!(blank_image(&opts(Drive::D1571)).unwrap().len(), 349_696);
        assert_eq!(blank_image(&opts(Drive::D1581)).unwrap().len(), 819_200);
        let d64 = blank_image(&opts(Drive::D1541)).unwrap();
        let bam = &d64[gcr_offset(18, 0)..][..BLOCK];
        let free: u32 = (1..=35).filter(|&t| t != 18).map(|t| bam[4 + (t - 1) * 4] as u32).sum();
        assert_eq!(free, Drive::D1541.blocks_free());
        // Byte for byte what VICE writes (c1541 -format "test disk,ab" d64).
        assert_eq!(&bam[..8], &[0x12, 0x01, 0x41, 0x00, 0x15, 0xff, 0xff, 0x1f]);
        assert_eq!(
            &bam[0x90..0xab],
            b"TEST DISK\xa0\xa0\xa0\xa0\xa0\xa0\xa0\xa0\xa0AB\xa02A\xa0\xa0\xa0\xa0"
        );
    }

    #[test]
    fn names() {
        assert!(validate_name("GAMES 1").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("SEVENTEEN CHARS!!").is_err());
        assert!(validate_name("A,B").is_err());
        assert!(validate_id("2A").is_ok());
        assert!(validate_id("A").is_err());
    }

    /// Byte-for-byte comparison with VICE's c1541 when it is installed.
    #[test]
    fn identical_to_vice_c1541() {
        let dir = std::env::temp_dir().join(format!("hgf-cbm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for d in Drive::ALL {
            let p = dir.join(format!("ref.{}", d.extension()));
            let ok = Command::new("c1541")
                .args(["-format", "test disk,ab", d.extension()])
                .arg(&p)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !ok {
                continue; // VICE not installed
            }
            let reference = std::fs::read(&p).unwrap();
            let ours = blank_image(&opts(d)).unwrap();
            assert_eq!(ours.len(), reference.len(), "{d:?}");
            if let Some(i) = (0..ours.len()).find(|&i| ours[i] != reference[i]) {
                panic!(
                    "{d:?}: first difference at byte {i:#x}: ours {:#04x}, c1541 {:#04x}",
                    ours[i], reference[i]
                );
            }
            // c1541 lists the blank disk with the expected number of free blocks.
            let out = Command::new("c1541").arg(&p).arg("-list").output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            assert!(text.contains(&format!("{} blocks free", d.blocks_free())), "{d:?}: {text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
