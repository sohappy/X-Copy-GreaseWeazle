// SPDX-License-Identifier: GPL-3.0-or-later

//! Blank Acorn DFS disk images (BBC Micro, Electron with DFS, Master): SSD and DSD.
//!
//! The catalogue is the two first sectors of track 0 of each side, as `*FORM` + `*TITLE`
//! write it; checked against dfsimage in the tests. Double-sided disks are two independent
//! DFS drives (side 1 = drive 0, side 2 = drive 2), stored track-interleaved in a DSD file.

const SECTOR: usize = 256;
const SPT: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DfsOpts {
    /// 40 or 80 tracks.
    pub tracks80: bool,
    pub double_sided: bool,
    /// Title of side 1 (and of side 2, with " 2" appended when room allows).
    pub title: String,
}

impl DfsOpts {
    pub fn tracks(&self) -> usize {
        if self.tracks80 {
            80
        } else {
            40
        }
    }

    pub fn gw_format(&self) -> &'static str {
        match (self.double_sided, self.tracks80) {
            (false, false) => "acorn.dfs.ss",
            (false, true) => "acorn.dfs.ss80",
            (true, false) => "acorn.dfs.ds",
            (true, true) => "acorn.dfs.ds80",
        }
    }

    pub fn extension(&self) -> &'static str {
        if self.double_sided {
            "dsd"
        } else {
            "ssd"
        }
    }

    pub fn label(&self) -> String {
        let kb = self.tracks() * SPT * SECTOR / 1024 * if self.double_sided { 2 } else { 1 };
        format!(
            "{} tracks, {} ({kb}K)",
            self.tracks(),
            if self.double_sided { "double-sided" } else { "single-sided" }
        )
    }

    /// Title of each side.
    pub fn side_titles(&self) -> Vec<String> {
        let t = self.title.trim().to_string();
        if !self.double_sided {
            return vec![t];
        }
        let second = if t.is_empty() {
            String::new()
        } else if t.chars().count() <= 10 {
            format!("{t} 2")
        } else {
            t.clone()
        };
        vec![t, second]
    }
}

pub fn validate_title(title: &str) -> Result<(), String> {
    if title.chars().count() > 12 {
        return Err("At most 12 characters".into());
    }
    if let Some(c) = title.chars().find(|c| !(' '..='~').contains(c) || *c == '"') {
        return Err(format!("Character '{c}' is not allowed in a DFS title"));
    }
    Ok(())
}

/// Write the empty catalogue of one side (sector 0 and 1 of track 0 of that side).
fn catalogue(s0: &mut [u8], s1: &mut [u8], title: &str, sectors: usize) {
    let t = title.as_bytes();
    let (first, rest) = t.split_at(t.len().min(8));
    s0[..first.len()].copy_from_slice(first);
    s1[..rest.len()].copy_from_slice(rest);
    s1[4] = 0; // catalogue cycle number (BCD)
    s1[5] = 0; // number of files × 8
    s1[6] = ((sectors >> 8) & 0x03) as u8; // boot option 0 (bits 4-5) + sector count bits 8-9
    s1[7] = (sectors & 0xff) as u8;
}

pub fn blank_image(o: &DfsOpts) -> Result<Vec<u8>, String> {
    validate_title(&o.title)?;
    let tracks = o.tracks();
    let sides = if o.double_sided { 2 } else { 1 };
    let track_bytes = SPT * SECTOR;
    let mut img = vec![0u8; tracks * sides * track_bytes];
    for (side, title) in o.side_titles().iter().enumerate() {
        // Interleaved DSD: track 0 side 0, track 0 side 1, track 1 side 0…
        let off = side * track_bytes;
        let (s0, s1) = img[off..off + 2 * SECTOR].split_at_mut(SECTOR);
        catalogue(s0, s1, title, tracks * SPT);
    }
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn layout() {
        let o = DfsOpts { tracks80: false, double_sided: false, title: "TESTDISK".into() };
        let img = blank_image(&o).unwrap();
        assert_eq!(img.len(), 102_400);
        assert_eq!(&img[..8], b"TESTDISK");
        assert_eq!(&img[256..264], &[0, 0, 0, 0, 0, 0, 0x01, 0x90]); // 400 sectors
        let o = DfsOpts { tracks80: true, double_sided: true, title: "SIDE".into() };
        let img = blank_image(&o).unwrap();
        assert_eq!(img.len(), 409_600);
        assert_eq!(&img[256 + 6..256 + 8], &[0x03, 0x20]); // 800 sectors
        assert_eq!(&img[2560..2566], b"SIDE 2");
        assert_eq!(o.gw_format(), "acorn.dfs.ds80");
    }

    #[test]
    fn titles() {
        assert!(validate_title("GAMES DISK 1").is_ok());
        assert!(validate_title("THIRTEEN CHRS").is_err());
        assert!(validate_title("A\"B").is_err());
        let o = DfsOpts { double_sided: true, title: "LONG TITLE 1".into(), ..Default::default() };
        assert_eq!(o.side_titles(), ["LONG TITLE 1", "LONG TITLE 1"]);
    }

    /// Compare the catalogue sectors with dfsimage, and let dfsimage read our images.
    #[test]
    fn agrees_with_dfsimage() {
        let dir = std::env::temp_dir().join(format!("hgf-dfs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (tracks80, ds) in [(false, false), (true, false), (false, true), (true, true)] {
            let o = DfsOpts { tracks80, double_sided: ds, title: "HGF TEST".into() };
            let ours = blank_image(&o).unwrap();
            let p = dir.join(format!("ref-{}.{}", o.tracks() * if ds { 2 } else { 1 }, o.extension()));
            let mut cmd = Command::new("dfsimage");
            cmd.args(["create", "--new", "--expand", "-q"]);
            cmd.arg(if tracks80 { "-80" } else { "-40" });
            cmd.arg(if ds { "-D" } else { "-S" });
            if ds {
                cmd.arg("-I");
            }
            for t in o.side_titles() {
                cmd.arg(format!("--title={t}"));
            }
            let Ok(out) = cmd.arg(&p).output() else { continue }; // dfsimage not installed
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            let reference = std::fs::read(&p).unwrap();
            assert_eq!(ours.len(), reference.len());
            for side in 0..o.side_titles().len() {
                let off = side * SPT * SECTOR;
                assert_eq!(&ours[off..off + 512], &reference[off..off + 512], "{tracks80} {ds} side {side}");
            }
            let mine = dir.join(format!("ours-{}.{}", o.tracks() * if ds { 2 } else { 1 }, o.extension()));
            std::fs::write(&mine, &ours).unwrap();
            let out = Command::new("dfsimage").args(["list"]).arg(&mine).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && text.contains("HGF TEST (00)"), "{text}");
            if ds {
                assert!(text.contains("HGF TEST 2 (00)") && text.contains("Drive 2"), "{text}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
