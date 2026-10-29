// SPDX-License-Identifier: GPL-3.0-or-later

//! Blank Apple II disk images (5.25", 140K): DOS 3.3 (.do) and ProDOS (.po).
//!
//! Data disks: the volume structures (DOS 3.3 VTOC and catalogue; ProDOS volume directory and
//! block bitmap) without the operating system, so the disk does not boot. Checked against
//! AppleCommander in the tests.

const SECTOR: usize = 256;
const TRACKS: usize = 35;
const SPT: usize = 16;
pub const IMAGE_SIZE: usize = TRACKS * SPT * SECTOR;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Os {
    #[default]
    Dos33,
    ProDos,
}

impl Os {
    pub fn label(self) -> &'static str {
        match self {
            Os::Dos33 => "DOS 3.3",
            Os::ProDos => "ProDOS",
        }
    }

    pub fn gw_format(self) -> &'static str {
        match self {
            Os::Dos33 => "apple2.appledos.140",
            Os::ProDos => "apple2.prodos.140",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Os::Dos33 => "do",
            Os::ProDos => "po",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppleOpts {
    pub os: Os,
    /// DOS 3.3 volume number (1-254).
    pub volume: u8,
    /// ProDOS volume name.
    pub name: String,
}

impl Default for AppleOpts {
    fn default() -> Self {
        Self { os: Os::Dos33, volume: 254, name: "BLANK".into() }
    }
}

pub fn validate_prodos_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Enter a volume name".into());
    }
    if n.len() > 15 {
        return Err("At most 15 characters".into());
    }
    if !n.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return Err("The name must start with a letter".into());
    }
    if let Some(c) = n.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '.')) {
        return Err(format!("Character '{c}' is not allowed (letters, digits and '.' only)"));
    }
    Ok(())
}

pub fn validate(o: &AppleOpts) -> Result<(), String> {
    match o.os {
        Os::Dos33 if o.volume == 0 || o.volume == 255 => Err("The volume number is 1 to 254".into()),
        Os::Dos33 => Ok(()),
        Os::ProDos => validate_prodos_name(&o.name),
    }
}

pub fn blank_image(o: &AppleOpts, now: u64) -> Result<Vec<u8>, String> {
    validate(o)?;
    Ok(match o.os {
        Os::Dos33 => dos33(o.volume),
        Os::ProDos => prodos(&o.name.trim().to_ascii_uppercase(), now),
    })
}

/// DOS 3.3 in DOS logical sector order (.do): VTOC at track 17 sector 0, catalogue in
/// sectors 15 down to 1. Track 0 (boot) and track 17 are in use; tracks 1-2, which INIT would
/// fill with DOS itself, are free on a data disk.
fn dos33(volume: u8) -> Vec<u8> {
    let mut img = vec![0u8; IMAGE_SIZE];
    let at = |t: usize, s: usize| (t * SPT + s) * SECTOR;
    let v = at(17, 0);
    {
        let vtoc = &mut img[v..v + SECTOR];
        vtoc[0x01] = 17; // first catalogue sector
        vtoc[0x02] = 15;
        vtoc[0x03] = 3; // DOS release
        vtoc[0x06] = volume;
        vtoc[0x27] = 122; // track/sector pairs per list sector
        vtoc[0x30] = 18; // last track allocated
        vtoc[0x31] = 1; // allocation direction
        vtoc[0x34] = TRACKS as u8;
        vtoc[0x35] = SPT as u8;
        vtoc[0x36] = 0x00; // bytes per sector, little endian
        vtoc[0x37] = 0x01;
        for t in 0..TRACKS {
            let free = t != 0 && t != 17;
            let e = 0x38 + t * 4;
            if free {
                vtoc[e] = 0xff;
                vtoc[e + 1] = 0xff;
            }
        }
    }
    for s in (1..=15).rev() {
        let c = at(17, s);
        if s > 1 {
            img[c + 1] = 17;
            img[c + 2] = (s - 1) as u8;
        }
    }
    img
}

/// ProDOS date and time words of `secs` since the Unix epoch (UTC).
fn prodos_datetime(secs: u64) -> [u8; 4] {
    let (y, mo, d, h, mi, _) = crate::fs::fat12::utc(secs);
    let date: u16 = (((y.rem_euclid(100)) as u16) << 9) | ((mo as u16) << 5) | d as u16;
    let [d0, d1] = date.to_le_bytes();
    [d0, d1, mi as u8, h as u8]
}

/// ProDOS in block order (.po): blocks 2-5 volume directory, block 6 bitmap.
fn prodos(name: &str, now: u64) -> Vec<u8> {
    const BLOCK: usize = 512;
    const BLOCKS: usize = IMAGE_SIZE / BLOCK; // 280
    let mut img = vec![0u8; IMAGE_SIZE];
    for b in 2..=5usize {
        let o = b * BLOCK;
        let prev = if b == 2 { 0 } else { b - 1 } as u16;
        let next = if b == 5 { 0 } else { b + 1 } as u16;
        img[o..o + 2].copy_from_slice(&prev.to_le_bytes());
        img[o + 2..o + 4].copy_from_slice(&next.to_le_bytes());
    }
    let h = &mut img[2 * BLOCK + 4..2 * BLOCK + 4 + 0x27];
    h[0] = 0xf0 | name.len() as u8; // storage type F: volume directory header
    h[1..1 + name.len()].copy_from_slice(name.as_bytes());
    h[0x18..0x1c].copy_from_slice(&prodos_datetime(now));
    h[0x1c] = 0; // version
    h[0x1d] = 0; // min version
    h[0x1e] = 0xc3; // access: destroy, rename, write, read
    h[0x1f] = 0x27; // entry length
    h[0x20] = 0x0d; // entries per block
    h[0x21..0x23].copy_from_slice(&0u16.to_le_bytes()); // file count
    h[0x23..0x25].copy_from_slice(&6u16.to_le_bytes()); // bitmap pointer
    h[0x25..0x27].copy_from_slice(&(BLOCKS as u16).to_le_bytes());
    // Bitmap: 1 = free, block n -> byte n/8, bit 7 - n%8. Blocks 0-6 are used.
    let bm = 6 * BLOCK;
    for n in 7..BLOCKS {
        img[bm + n / 8] |= 0x80 >> (n % 8);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const NOW: u64 = 1_792_261_050; // 2026-10-17 18:17:30 UTC

    #[test]
    fn dos33_vtoc() {
        let img = blank_image(&AppleOpts::default(), NOW).unwrap();
        assert_eq!(img.len(), 143_360);
        let v = 17 * 16 * 256;
        assert_eq!(&img[v..v + 8], &[0x00, 0x11, 0x0f, 0x03, 0x00, 0x00, 0xfe, 0x00]);
        // Free sectors: 33 tracks × 16 = 528, what CATALOG-based tools report.
        let free: u32 = (0..35)
            .map(|t| {
                u32::from(img[v + 0x38 + t * 4]).count_ones() + u32::from(img[v + 0x39 + t * 4]).count_ones()
            })
            .sum();
        assert_eq!(free, 528);
    }

    #[test]
    fn prodos_header() {
        let o = AppleOpts { os: Os::ProDos, name: "test.disk".into(), ..Default::default() };
        let img = blank_image(&o, NOW).unwrap();
        let h = &img[1024..1024 + 0x2c];
        assert_eq!(&h[..4], &[0, 0, 3, 0]);
        assert_eq!(&h[4..14], b"\xf9TEST.DISK");
        assert_eq!(&h[0x1c..0x20], &[0x51, 0x35, 17, 18]); // 2026-10-17 18:17
        assert_eq!(&img[6 * 512..6 * 512 + 2], &[0x01, 0xff]);
        assert!(validate_prodos_name("9LIVES").is_err());
        assert!(validate_prodos_name("MY DISK").is_err());
    }

    /// AppleCommander (path in HGF_APPLECOMMANDER, a jar) reads our images, and the structures
    /// match the ones it creates. Skipped when it is not available.
    #[test]
    fn agrees_with_applecommander() {
        let Ok(jar) = std::env::var("HGF_APPLECOMMANDER") else { return };
        let dir = std::env::temp_dir().join(format!("hgf-a2-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ac = |args: &[&str]| {
            let out = Command::new("java").arg("-jar").arg(&jar).args(args).output().unwrap();
            String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)
        };
        let path = |n: &str| dir.join(n).to_string_lossy().into_owned();

        // DOS 3.3: everything but the boot track must be identical.
        ac(&["-dos140", &path("ref.do")]);
        let reference = std::fs::read(path("ref.do")).unwrap();
        let ours = blank_image(&AppleOpts::default(), NOW).unwrap();
        assert_eq!(&ours[16 * 256..], &reference[16 * 256..], "DOS 3.3 tracks 1-34");
        std::fs::write(path("ours.do"), &ours).unwrap();
        let info = ac(&["-i", &path("ours.do")]);
        assert!(info.contains("Disk Format: DOS 3.3") && info.contains("Free Sectors: 528"), "{info}");

        // ProDOS: identical apart from the boot block, the creation date and the access byte.
        ac(&["-pro140", &path("ref.po"), "TEST.DISK"]);
        let mut reference = std::fs::read(path("ref.po")).unwrap();
        let o = AppleOpts { os: Os::ProDos, name: "TEST.DISK".into(), ..Default::default() };
        let mut ours = blank_image(&o, NOW).unwrap();
        std::fs::write(path("ours.po"), &ours).unwrap();
        for img in [&mut reference, &mut ours] {
            img[1024 + 0x1c..1024 + 0x20].fill(0);
            img[1024 + 0x22] = 0;
        }
        assert_eq!(&ours[1024..], &reference[1024..], "ProDOS blocks 2-279");
        let info = ac(&["-i", &path("ours.po")]);
        assert!(info.contains("Volume Name: TEST.DISK") && info.contains("Free Blocks: 273"), "{info}");
        assert!(info.contains("Creation Date: 10/17/2026"), "{info}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
