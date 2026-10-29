// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the Greaseweazle host tools (`gw`) without user configuration.
//!
//! Candidates are checked in order and each one is validated by running `gw --help`
//! (which prints the list of actions and does not touch the USB device).

use crate::gw::runner;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(windows)]
const EXE: &str = "gw.exe";
#[cfg(not(windows))]
const EXE: &str = "gw";

/// Whether `program` is a working gw.
pub fn is_gw(program: &str) -> bool {
    match runner::capture(program, &["--help"], Duration::from_secs(15)) {
        Ok(out) => out.contains("Actions:") || (out.contains("Usage:") && out.contains("[action]")),
        Err(_) => false,
    }
}

/// Every place where gw is commonly installed, most likely first. Paths are not validated.
pub fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };

    // 1. On the PATH.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            push(dir.join(EXE));
        }
    }

    // 2. Next to HappyGreaseFloppy (portable bundle), including an unzipped release folder.
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        push(dir.join(EXE));
        for d in subdirs(&dir, "greaseweazle") {
            push(d.join(EXE));
        }
    }

    let home = home_dir();

    // 3. Python user installs.
    if let Some(h) = &home {
        push(h.join(".local/bin").join(EXE));
        push(h.join(".local/pipx/venvs/greaseweazle/bin").join(EXE));
        push(h.join(".local/share/pipx/venvs/greaseweazle/bin").join(EXE));
        for d in subdirs(&h.join("Library/Python"), "") {
            push(d.join("bin").join(EXE));
        }
    }
    // python.org installer on macOS: /Library/Frameworks/Python.framework/Versions/3.x/bin
    for d in subdirs(Path::new("/Library/Frameworks/Python.framework/Versions"), "") {
        push(d.join("bin").join(EXE));
    }
    for p in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        push(Path::new(p).join(EXE));
    }

    // 4. Windows: Python Scripts folders and the official zip unpacked in usual places.
    #[cfg(windows)]
    {
        for var in ["LOCALAPPDATA", "APPDATA"] {
            if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
                for py in [base.join("Programs/Python"), base.join("Python")] {
                    for d in subdirs(&py, "Python") {
                        push(d.join("Scripts").join(EXE));
                    }
                }
            }
        }
        if let Some(h) = &home {
            for dir in ["Downloads", "Desktop", "Documents"] {
                for d in subdirs(&h.join(dir), "greaseweazle") {
                    push(d.join(EXE));
                    for inner in subdirs(&d, "greaseweazle") {
                        push(inner.join(EXE));
                    }
                }
            }
        }
        for d in subdirs(Path::new("C:\\"), "greaseweazle") {
            push(d.join(EXE));
        }
    }
    out
}

/// Find a working gw: `preferred` first, then the usual locations.
pub fn detect(preferred: &str) -> Option<String> {
    let preferred = preferred.trim();
    if !preferred.is_empty() && is_gw(preferred) {
        return Some(preferred.to_string());
    }
    candidates()
        .into_iter()
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .find(|p| is_gw(p))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Sub-directories of `dir` whose name starts with `prefix` (case-insensitive), sorted newest
/// name last so that e.g. `greaseweazle-1.23` wins over `greaseweazle-1.10` when reversed.
fn subdirs(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let prefix = prefix.to_lowercase();
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    v.sort();
    v.reverse();
    v
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake(dir: &Path, body: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("gw");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn validates_candidates() {
        let base = std::env::temp_dir().join(format!("hgf-locate-{}", std::process::id()));
        let good = fake(&base.join("good"), "echo 'Usage: gw [--time] [action] [-h] ...'; echo 'Actions:'");
        let bad = fake(&base.join("bad"), "echo hello");
        assert!(is_gw(good.to_str().unwrap()));
        assert!(!is_gw(bad.to_str().unwrap()));
        assert!(!is_gw("/nonexistent/gw"));
        assert_eq!(detect(good.to_str().unwrap()).as_deref(), good.to_str());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn subdirs_are_filtered() {
        let base = std::env::temp_dir().join(format!("hgf-subdirs-{}", std::process::id()));
        for d in ["greaseweazle-1.10", "Greaseweazle-1.23", "other"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let found = subdirs(&base, "greaseweazle");
        assert_eq!(found.len(), 2);
        std::fs::remove_dir_all(&base).ok();
    }
}
