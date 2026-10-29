// SPDX-License-Identifier: GPL-3.0-or-later

//! Image file options: gw's `file.ext::opt=val:opt=val` suffix (`tools/util.py split_opts`).
//!
//! The options and their values are those of the gw image classes (`HFEOpts`, `SCPOpts`,
//! `KFOpts`, `D88Opts`). Options for a file gw writes are `w_settings`; for a file it reads,
//! `r_settings`.

use std::ffi::OsString;
use std::path::Path;

/// One option of an image type.
#[derive(Debug, PartialEq, Eq)]
pub struct OptSpec {
    pub key: &'static str,
    /// Accepted values; empty = free value (number).
    pub values: &'static [&'static str],
    pub help: &'static str,
    /// A yes/no flag (`opt` alone means yes).
    pub flag: bool,
}

const fn opt(key: &'static str, values: &'static [&'static str], help: &'static str) -> OptSpec {
    OptSpec { key, values, help, flag: false }
}

const fn flag(key: &'static str, help: &'static str) -> OptSpec {
    OptSpec { key, values: &[], help, flag: true }
}

const HFE_INTERFACES: &[&str] = &[
    "ibmpc_dd",
    "ibmpc_hd",
    "atarist_dd",
    "atarist_hd",
    "amiga_dd",
    "amiga_hd",
    "cpc_dd",
    "generic_shugart_dd",
    "ibmpc_ed",
    "msx2_dd",
    "c64_dd",
    "emu_shugart",
    "s950_dd",
    "s950_hd",
    "s950_dd_hd",
    "ibmpc_dd_hd",
    "quickdisk",
    "unknown",
];

const HFE_ENCODINGS: &[&str] = &[
    "isoibm_mfm",
    "amiga_mfm",
    "isoibm_fm",
    "emu_fm",
    "tycom_fm",
    "membrain_mfm",
    "appleii_gcr1",
    "appleii_gcr2",
    "appleii_hddd_a2_gcr1",
    "appleii_hddd_a2_gcr2",
    "arburgdat",
    "arburgsys",
    "aed6200p_mfm",
    "northstar_hs_mfm",
    "heathkit_hs_fm",
    "dec_rx02_m2fm",
    "applemac_gcr",
    "qd_mo5",
    "c64_gcr",
    "victor9k_gcr",
    "micraln_hs_fm",
    "unknown",
];

const SCP_DISKTYPES: &[&str] = &[
    "c64",
    "amiga",
    "amigahd",
    "atari800-sd",
    "atari800-dd",
    "atari800-ed",
    "atarist-ss",
    "atarist-ds",
    "appleii",
    "appleiipro",
    "apple-400k",
    "apple-800k",
    "apple-1m44",
    "ibmpc-360k",
    "ibmpc-720k",
    "ibmpc-1m2",
    "ibmpc-1m44",
    "trs80_sssd",
    "trs80_ssdd",
    "trs80_dssd",
    "trs80_dsdd",
    "ti-99/4a",
    "roland-d20",
    "amstrad-cpc",
    "other-320k",
    "other-1m2",
    "other-720k",
    "other-1m44",
    "tape-gcr1",
    "tape-gcr2",
    "tape-mfm",
    "hdd-mfm",
    "hdd-rll",
];

const HFE: &[OptSpec] = &[
    opt(
        "bitrate",
        &["125", "250", "300", "500", "1000"],
        "kbit/s: 250 for DD, 500 for HD (required from flux)",
    ),
    opt("version", &["1", "3"], "HFE v1 (default, every HxC/Gotek) or v3 (HFEv3, weak bits)"),
    opt("interface", HFE_INTERFACES, "interface mode stored in the HFE header (for HxC/FlashFloppy)"),
    opt("encoding", HFE_ENCODINGS, "track encoding stored in the HFE header"),
    flag("double_step", "the image is for a 40-track disk in an 80-track drive"),
    flag("uniform", "same bit length on every track"),
];

const SCP: &[OptSpec] = &[
    opt("disktype", SCP_DISKTYPES, "disk type stored in the SCP header"),
    opt("revs", &[], "number of revolutions to keep per track"),
    flag("legacy_ss", "old (incorrect) single-sided SCP layout, for old tools"),
];

const KRYOFLUX: &[OptSpec] = &[
    opt("sck", &[], "sample clock, e.g. 72m (MHz)"),
    opt("revs", &[], "number of revolutions to keep per track"),
];

const D88: &[OptSpec] = &[opt("index", &[], "disk number inside a multi-disk D88 file (0 = first)")];

/// Options gw accepts for an image file it writes (read destination, convert output).
pub fn for_output(path: &Path) -> &'static [OptSpec] {
    match crate::disk::formats::extension(path).as_deref() {
        Some("hfe") => HFE,
        Some("scp") => SCP,
        Some("raw") => KRYOFLUX,
        _ => &[],
    }
}

/// Options gw accepts for an image file it reads (write source, convert input).
pub fn for_input(path: &Path) -> &'static [OptSpec] {
    match crate::disk::formats::extension(path).as_deref() {
        Some("d88") => D88,
        _ => &[],
    }
}

/// Parsed `opt=val:opt` list (gw's syntax; `::` separators are accepted too).
pub fn parse(opts: &str) -> Vec<(String, String)> {
    opts.split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| match s.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (s.to_string(), "yes".to_string()),
        })
        .collect()
}

fn join(list: &[(String, String)]) -> String {
    list.iter()
        .map(|(k, v)| if v == "yes" { k.clone() } else { format!("{k}={v}") })
        .collect::<Vec<_>>()
        .join(":")
}

/// Set (or, with an empty value, remove) one option.
pub fn set(opts: &mut String, key: &str, value: &str) {
    let mut list = parse(opts);
    list.retain(|(k, _)| k != key);
    if !value.is_empty() {
        list.push((key.to_string(), value.to_string()));
    }
    *opts = join(&list);
}

pub fn get(opts: &str, key: &str) -> Option<String> {
    parse(opts).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Check the options against what gw accepts for this file.
pub fn validate(specs: &[OptSpec], path: &Path, opts: &str) -> Result<(), String> {
    let list = parse(opts);
    if list.is_empty() {
        return Ok(());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if specs.is_empty() {
        return Err(format!("{name}: this image type takes no file options"));
    }
    for (k, v) in &list {
        let Some(s) = specs.iter().find(|s| s.key == k) else {
            let valid: Vec<_> = specs.iter().map(|s| s.key).collect();
            return Err(format!("{name}: unknown file option '{k}' (valid: {})", valid.join(", ")));
        };
        if !s.values.is_empty() && !s.values.iter().any(|x| x.eq_ignore_ascii_case(v)) {
            return Err(format!("{name}: '{v}' is not a valid {k}"));
        }
        if s.values.is_empty() && !s.flag && k != "sck" && v.parse::<u32>().is_err() {
            return Err(format!("{name}: {k} must be a number"));
        }
    }
    Ok(())
}

/// The path argument for gw: `path` or `path::opts`.
pub fn with_opts(path: &Path, opts: &str) -> OsString {
    let list = parse(opts);
    let mut s = path.as_os_str().to_os_string();
    if !list.is_empty() {
        s.push("::");
        s.push(join(&list));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn syntax_matches_gw() {
        let p = PathBuf::from("/x/disk.hfe");
        assert_eq!(with_opts(&p, ""), OsString::from("/x/disk.hfe"));
        assert_eq!(
            with_opts(&p, "bitrate=250:version=3"),
            OsString::from("/x/disk.hfe::bitrate=250:version=3")
        );
        assert_eq!(
            with_opts(&p, " bitrate = 250 :: double_step "),
            OsString::from("/x/disk.hfe::bitrate=250:double_step")
        );
    }

    #[test]
    fn set_and_get() {
        let mut o = String::new();
        set(&mut o, "bitrate", "250");
        set(&mut o, "version", "3");
        set(&mut o, "bitrate", "500");
        assert_eq!(o, "version=3:bitrate=500");
        set(&mut o, "version", "");
        assert_eq!(o, "bitrate=500");
        assert_eq!(get(&o, "bitrate").as_deref(), Some("500"));
        set(&mut o, "uniform", "yes");
        assert_eq!(o, "bitrate=500:uniform");
    }

    #[test]
    fn validation() {
        let hfe = PathBuf::from("a.hfe");
        assert!(validate(for_output(&hfe), &hfe, "bitrate=250:interface=AMIGA_DD:double_step").is_ok());
        assert!(validate(for_output(&hfe), &hfe, "bitrate=fast").is_err());
        assert!(validate(for_output(&hfe), &hfe, "disktype=amiga").unwrap_err().contains("unknown"));
        let scp = PathBuf::from("a.scp");
        assert!(validate(for_output(&scp), &scp, "disktype=amiga:revs=2").is_ok());
        let adf = PathBuf::from("a.adf");
        assert!(validate(for_output(&adf), &adf, "").is_ok());
        assert!(validate(for_output(&adf), &adf, "x=1").unwrap_err().contains("no file options"));
        let raw = PathBuf::from("track00.0.raw");
        assert!(validate(for_output(&raw), &raw, "sck=72m").is_ok());
        let d88 = PathBuf::from("multi.d88");
        assert!(validate(for_input(&d88), &d88, "index=2").is_ok());
    }
}
