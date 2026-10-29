// SPDX-License-Identifier: GPL-3.0-or-later

//! Automatic naming of read images: `disk_001.adf`, `disk_002.adf`… for archiving a box of disks.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Numbering {
    pub enabled: bool,
    /// Number of the next disk.
    pub next: u32,
    /// Zero-padded width (1-6).
    pub digits: u8,
}

impl Default for Numbering {
    fn default() -> Self {
        Self { enabled: false, next: 1, digits: 3 }
    }
}

impl Numbering {
    /// `dir/name.ext` → `dir/name_007.ext` (the number replaces a trailing `_NNN` already there).
    pub fn apply(&self, path: &Path) -> PathBuf {
        if !self.enabled {
            return path.to_path_buf();
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let base = match stem.rsplit_once('_') {
            Some((b, n)) if !b.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
                b.to_string()
            }
            _ => stem,
        };
        let base = if base.is_empty() { "disk".to_string() } else { base };
        let digits = self.digits.clamp(1, 6) as usize;
        let mut name = format!("{base}_{:0digits$}", self.next);
        if let Some(ext) = path.extension() {
            name.push('.');
            name.push_str(&ext.to_string_lossy());
        }
        path.with_file_name(name)
    }

    /// After a successful read.
    pub fn advance(&mut self) {
        if self.enabled {
            self.next = self.next.saturating_add(1);
        }
    }
}

/// File name made from a disk or volume name: characters forbidden on Windows or macOS
/// become `_`; an empty result becomes `disk`.
pub fn safe_file_stem(name: &str) -> String {
    let s: String =
        name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || (c as u32) < 32 { '_' } else { c }).collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        "disk".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_stems() {
        assert_eq!(safe_file_stem("A/B:C?"), "A_B_C_");
        assert_eq!(safe_file_stem("  "), "disk");
        assert_eq!(safe_file_stem("Workbench 1.3"), "Workbench 1.3");
    }

    #[test]
    fn numbered_names() {
        let mut n = Numbering { enabled: true, next: 7, digits: 3 };
        assert_eq!(n.apply(Path::new("/a/games.adf")), PathBuf::from("/a/games_007.adf"));
        assert_eq!(n.apply(Path::new("/a/games_006.adf")), PathBuf::from("/a/games_007.adf"));
        assert_eq!(n.apply(Path::new("/a/my_disk.scp")), PathBuf::from("/a/my_disk_007.scp"));
        n.digits = 1;
        n.next = 12;
        assert_eq!(n.apply(Path::new("x.img")), PathBuf::from("x_12.img"));
        n.advance();
        assert_eq!(n.next, 13);
        n.enabled = false;
        assert_eq!(n.apply(Path::new("x.img")), PathBuf::from("x.img"));
        n.advance();
        assert_eq!(n.next, 13);
    }
}
