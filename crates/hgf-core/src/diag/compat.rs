// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks before writing an Amiga disk: options that gw's own verification cannot catch
//! (the disk reads back fine in the same drive with the same options, but not on an Amiga)
//! and images that need a newer Kickstart than the target machine has.

use crate::fs::amigados::AdfInfo;
use crate::gw::{Densel, WriteOpts};

/// Write precompensation used by the Amiga's trackdisk.device on DD disks.
pub const AMIGA_PRECOMP: &str = "40=140";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The disk will probably not work on the Amiga.
    Warn,
    /// Worth knowing, not an error.
    Info,
    /// Explicitly fine.
    Good,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concern {
    pub severity: Severity,
    pub text: String,
    /// Fixed by resetting the write options (see [`reset_risky`]).
    pub option: bool,
}

fn warn(text: &str, option: bool) -> Concern {
    Concern { severity: Severity::Warn, text: text.into(), option }
}

/// Is this write aimed at an Amiga disk?
pub fn is_amiga_write(o: &WriteOpts) -> bool {
    if !o.format.is_empty() {
        return o.format.starts_with("amiga.");
    }
    crate::disk::formats::extension(&o.input).as_deref() == Some("adf")
}

/// Everything that may make the written disk unreadable on an Amiga.
///
/// `image` is the result of [`crate::fs::amigados::inspect_adf`] on the source image, if it was read.
pub fn amiga_write_concerns(o: &WriteOpts, image: Option<&Result<AdfInfo, String>>) -> Vec<Concern> {
    let mut out = Vec::new();
    if !is_amiga_write(o) {
        return out;
    }
    // Options: gw verifies with the same settings, so the disk "passes" and still fails on the Amiga.
    if o.tracks.hswap {
        out.push(warn("Swap heads is on: each side is written on the other one.", true));
    }
    if o.tracks.double_step {
        out.push(warn(
            "Double step is on (40-track 5.25\" disks only): the Amiga will not find its tracks.",
            true,
        ));
    }
    if !o.tracks.custom.trim().is_empty() {
        out.push(warn(
            "Custom tracks are set (head offset, cylinder range…): only the format default makes a complete Amiga disk.",
            true,
        ));
    }
    if o.reverse {
        out.push(warn("Reverse track data is on (flippy disks only).", true));
    }
    if o.hard_sectors {
        out.push(warn("Hard-sectored is on: Amiga disks are soft-sectored.", true));
    }
    if !o.fake_index.trim().is_empty() {
        out.push(warn("Fake index is set: not needed for Amiga disks.", true));
    }
    if o.densel != Densel::Auto {
        out.push(warn(
            "Density select is forced: leave it on Auto, the drive picks DD from the disk's holes.",
            true,
        ));
    }
    if o.format == "amiga.amigados_hd" {
        out.push(warn(
            "HD format (1.76M): only HD Amiga drives read it (A4000, some A1200); not an A500 or A600.",
            false,
        ));
    }
    match image {
        Some(Ok(adf)) => {
            let fs = adf.filesystem.as_str();
            let kickstart = if fs.contains("Long names") {
                Some("3.2")
            } else if fs.contains("DirCache") {
                Some("3.0")
            } else if fs == "OFS" {
                None
            } else {
                Some("2.0") // FFS, OFS/FFS International, unknown types
            };
            if let Some(ks) = kickstart {
                out.push(Concern {
                    severity: Severity::Warn,
                    text: format!(
                        "The image is {fs}: it needs Kickstart {ks} or later. An A500 with \
                         Kickstart 1.3 shows \"Not a DOS disk\" (the write itself is fine)."
                    ),
                    option: false,
                });
            } else if adf.bootable {
                out.push(Concern {
                    severity: Severity::Good,
                    text: "OFS, bootable: works with Kickstart 1.3 (A500).".into(),
                    option: false,
                });
            } else {
                out.push(Concern {
                    severity: Severity::Info,
                    text: "OFS data disk, not bootable: start Workbench first, then insert it.".into(),
                    option: false,
                });
            }
        }
        Some(Err(e)) if e.starts_with("No AmigaDOS boot block") => out.push(Concern {
            severity: Severity::Info,
            text:
                "Non-DOS disk (custom loader, as most games): it boots by itself; Workbench cannot open it."
                    .into(),
            option: false,
        }),
        Some(Err(e)) => out.push(Concern { severity: Severity::Warn, text: format!("{e}."), option: false }),
        None => {}
    }
    out
}

/// Reset the options flagged by [`amiga_write_concerns`] (`option: true`).
pub fn reset_risky(o: &mut WriteOpts) {
    o.tracks = Default::default();
    o.reverse = false;
    o.hard_sectors = false;
    o.fake_index.clear();
    o.densel = Densel::Auto;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn adf(fs: &str, bootable: bool) -> Result<AdfInfo, String> {
        Ok(AdfInfo { filesystem: fs.into(), bootable, ..Default::default() })
    }

    fn opts() -> WriteOpts {
        WriteOpts { input: PathBuf::from("game.adf"), format: "amiga.amigados".into(), ..Default::default() }
    }

    fn warnings(c: &[Concern]) -> usize {
        c.iter().filter(|c| c.severity == Severity::Warn).count()
    }

    #[test]
    fn clean_ofs_write_is_fine() {
        let c = amiga_write_concerns(&opts(), Some(&adf("OFS", true)));
        assert_eq!(warnings(&c), 0);
        assert_eq!(c[0].severity, Severity::Good);
    }

    #[test]
    fn leftover_options_are_flagged_and_reset() {
        let mut o = opts();
        o.tracks.hswap = true;
        o.reverse = true;
        o.densel = Densel::High;
        o.precomp = AMIGA_PRECOMP.into();
        let c = amiga_write_concerns(&o, None);
        assert_eq!(warnings(&c), 3);
        assert!(c.iter().all(|c| c.option));
        reset_risky(&mut o);
        assert!(amiga_write_concerns(&o, None).is_empty());
        assert_eq!(o.precomp, AMIGA_PRECOMP, "precompensation is a deliberate choice, kept");
    }

    #[test]
    fn kickstart_requirements() {
        let text = |fs| amiga_write_concerns(&opts(), Some(&adf(fs, true)))[0].text.clone();
        assert!(text("FFS").contains("Kickstart 2.0"));
        assert!(text("OFS International").contains("Kickstart 2.0"));
        assert!(text("FFS DirCache").contains("Kickstart 3.0"));
        let nd = Err::<AdfInfo, _>("No AmigaDOS boot block (NDOS or non-DOS disk)".to_string());
        assert_eq!(amiga_write_concerns(&opts(), Some(&nd))[0].severity, Severity::Info);
    }

    #[test]
    fn only_for_amiga_writes() {
        let mut o = opts();
        o.format = "ibm.1440".into();
        o.tracks.hswap = true;
        assert!(amiga_write_concerns(&o, None).is_empty());
        o.format.clear(); // auto: from the .adf extension
        assert!(is_amiga_write(&o));
    }
}
