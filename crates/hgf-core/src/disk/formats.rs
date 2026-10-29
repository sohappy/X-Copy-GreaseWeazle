// SPDX-License-Identifier: GPL-3.0-or-later

//! Disk formats and image file types known to Greaseweazle.
//!
//! The authoritative list comes from the installed `gw` (parsed from `gw read --help`, see
//! [`parse_help`]); an embedded copy is used until then or if `gw` is unavailable.

use std::path::Path;

/// Formats of Greaseweazle host tools v1.x (used as a fallback).
const FALLBACK_FORMATS: &str = "
    acorn.adfs.160 acorn.adfs.1600 acorn.adfs.320 acorn.adfs.640 acorn.adfs.800 acorn.dfs.ds
    acorn.dfs.ds80 acorn.dfs.ss acorn.dfs.ss80 akai.1600 akai.800 amiga.amigados amiga.amigados_hd
    apple2.appledos.140 apple2.nofs.140 apple2.prodos.140 apricot.double apricot.single atari.130
    atari.90 atarist.360 atarist.400 atarist.440 atarist.720 atarist.800 atarist.880 coco.decb
    coco.decb.40t coco.os9.40ds coco.os9.40ss coco.os9.80ds coco.os9.80ss commodore.1541
    commodore.1571 commodore.1581 commodore.cmd.fd2000.dd commodore.cmd.fd2000.hd
    commodore.cmd.fd4000.ed datageneral.2f dec.rx01 dec.rx02 dec.rx50 dragon.40ds dragon.40ss
    dragon.80ds dragon.80ss eagle.dsqd.800 eagle.ssqd.400 ensoniq.1600 ensoniq.800 ensoniq.mirage
    epson.qx10.320 epson.qx10.396 epson.qx10.399 epson.qx10.400 epson.qx10.booter epson.qx10.logo
    gem.1600 hp.mmfm.9885 hp.mmfm.9895 ibm.1200 ibm.1440 ibm.160 ibm.1680 ibm.180 ibm.2880 ibm.320
    ibm.360 ibm.720 ibm.800 ibm.dmf ibm.scan kaypro.dsdd.40 kaypro.dsdd.80 kaypro.ssdd.40
    luxor.1000.abcnet luxor.1000.data luxor.1000.program luxor.160 luxor.320 luxor.640 luxor.80
    mac.400 mac.800 micropolis.100tpi.ds micropolis.100tpi.ds.275 micropolis.100tpi.ss
    micropolis.100tpi.ss.275 micropolis.48tpi.ds micropolis.48tpi.ds.275 micropolis.48tpi.ss
    micropolis.48tpi.ss.275 mm1.os9.80dshd_32 mm1.os9.80dshd_33 mm1.os9.80dshd_36
    mm1.os9.80dshd_37 msx.1d msx.1dd msx.2d msx.2dd northstar.fm.ds northstar.fm.ss
    northstar.mfm.ds northstar.mfm.ss occ1.dd occ1.sd olivetti.m20 pc98.2d pc98.2dd pc98.2hd
    pc98.2hs pc98.n88basic.hd raw.125 raw.250 raw.500 rm.5in.dd rm.5in.qd rm.5in.sd rm.8in.sd
    sci.prophet sega.sf7000 sharp.2d thomson.1s160 thomson.1s320 thomson.1s80 thomson.2s160
    thomson.2s320 tsc.flex.dsdd tsc.flex.ssdd xerox.860.dssd xerox.860.ss zx.3dos.ds80
    zx.3dos.ss40 zx.d80.ds80 zx.fdd3000.ds80 zx.fdd3000.ss40 zx.kempston.ds80 zx.kempston.ss40
    zx.opus.ds80 zx.opus.ss40 zx.plusd.ds80 zx.quorum.ds80 zx.rocky.ds80 zx.rocky.ss40
    zx.trdos.ds80 zx.turbodrive.ds40 zx.turbodrive.ds80 zx.watford.ds80 zx.watford.ss40";

const FALLBACK_SUFFIXES: &str = ".2d .a2r .adf .ads .adm .adl .ctr .d1m .d2m .d4m .d64 .d71 .d81 \
    .d88 .dcp .dim .dmk .do .dsd .dsk .edsk .fd .fdi .hdm .hfe .ima .img .imd .ipf .mgt .msa .nfd \
    .nsi .po .raw .sf7 .scp .ssd .st .td0 .xdf";

/// Image suffixes that store raw flux (no format needed to read them).
pub const FLUX_SUFFIXES: &[&str] = &["scp", "raw", "a2r", "ctr", "ipf"];

/// Image suffixes whose container has no geometry of its own: a `--format` is mandatory.
pub const NEEDS_FORMAT_SUFFIXES: &[&str] = &["img", "ima", "st"];

/// Formats and suffixes supported by the installed gw.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub formats: Vec<String>,
    pub suffixes: Vec<String>,
    /// `true` when the lists were read from the installed `gw`.
    pub from_gw: bool,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            formats: FALLBACK_FORMATS.split_whitespace().map(String::from).collect(),
            suffixes: FALLBACK_SUFFIXES
                .split_whitespace()
                .map(|s| s.trim_start_matches('.').to_string())
                .collect(),
            from_gw: false,
        }
    }
}

impl Catalog {
    pub fn knows_format(&self, f: &str) -> bool {
        self.formats.iter().any(|x| x == f)
    }

    pub fn knows_suffix(&self, path: &Path) -> bool {
        match extension(path) {
            Some(ext) => self.suffixes.contains(&ext),
            None => false,
        }
    }

    /// Formats grouped by family (`amiga`, `ibm`, …) in display order.
    pub fn families(&self) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for f in &self.formats {
            let fam = f.split('.').next().unwrap_or(f).to_string();
            match out.iter_mut().find(|(n, _)| *n == fam) {
                Some((_, v)) => v.push(f.clone()),
                None => out.push((fam, vec![f.clone()])),
            }
        }
        out.sort_by_key(|(f, _)| family_name(f).to_lowercase());
        out
    }
}

/// Parse the output of `gw read --help` into a catalog.
pub fn parse_help(text: &str) -> Option<Catalog> {
    let mut formats = Vec::new();
    let mut suffixes = Vec::new();
    let mut section = 0;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("FORMAT options") {
            section = 1;
            continue;
        }
        if t.starts_with("Supported file suffixes") {
            section = 2;
            continue;
        }
        if t.is_empty() {
            if section == 1 && !formats.is_empty() {
                section = 0;
            }
            continue;
        }
        match section {
            1 => formats.extend(t.split_whitespace().map(String::from)),
            2 => suffixes.extend(
                t.split_whitespace()
                    .filter(|s| s.starts_with('.'))
                    .map(|s| s.trim_start_matches('.').to_string()),
            ),
            _ => {}
        }
    }
    (!formats.is_empty()).then_some(Catalog {
        formats,
        suffixes: if suffixes.is_empty() { Catalog::default().suffixes } else { suffixes },
        from_gw: true,
    })
}

/// Lower-case extension of a path, without the dot.
pub fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
}

/// Human-readable name of a format family prefix.
pub fn family_name(prefix: &str) -> &str {
    match prefix {
        "acorn" => "Acorn (BBC / Archimedes)",
        "akai" => "Akai samplers",
        "amiga" => "Commodore Amiga",
        "apple2" => "Apple II",
        "apricot" => "Apricot",
        "atari" => "Atari 8-bit",
        "atarist" => "Atari ST",
        "coco" => "Tandy CoCo",
        "commodore" => "Commodore 8-bit",
        "datageneral" => "Data General",
        "dec" => "DEC",
        "dragon" => "Dragon",
        "eagle" => "Eagle",
        "ensoniq" => "Ensoniq",
        "epson" => "Epson QX-10",
        "gem" => "GEM samplers",
        "hp" => "Hewlett-Packard",
        "ibm" => "IBM PC & compatibles",
        "kaypro" => "Kaypro",
        "luxor" => "Luxor ABC",
        "mac" => "Apple Macintosh",
        "micropolis" => "Micropolis",
        "mm1" => "MM/1 (OS-9)",
        "msx" => "MSX",
        "northstar" => "North Star",
        "occ1" => "Osborne 1",
        "olivetti" => "Olivetti",
        "pc98" => "NEC PC-98",
        "raw" => "Raw bitcell",
        "rm" => "Research Machines",
        "sci" => "Sequential Circuits",
        "sega" => "Sega",
        "sharp" => "Sharp",
        "thomson" => "Thomson",
        "tsc" => "TSC FLEX",
        "xerox" => "Xerox",
        "zx" => "Sinclair ZX Spectrum",
        other => other,
    }
}

/// A one-click machine preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub label: &'static str,
    /// Empty string = raw flux, no decoding.
    pub format: &'static str,
    pub extension: &'static str,
    pub cyls: u16,
    pub heads: u8,
}

pub const PRESETS: &[Preset] = &[
    Preset { label: "Amiga DD 880K", format: "amiga.amigados", extension: "adf", cyls: 80, heads: 2 },
    Preset { label: "Amiga HD 1.76M", format: "amiga.amigados_hd", extension: "adf", cyls: 80, heads: 2 },
    Preset { label: "Atari ST 720K", format: "atarist.720", extension: "st", cyls: 80, heads: 2 },
    Preset { label: "Atari ST 360K", format: "atarist.360", extension: "st", cyls: 80, heads: 1 },
    Preset { label: "PC 1.44M", format: "ibm.1440", extension: "img", cyls: 80, heads: 2 },
    Preset { label: "PC 720K", format: "ibm.720", extension: "img", cyls: 80, heads: 2 },
    Preset { label: "PC 1.2M (5.25\")", format: "ibm.1200", extension: "img", cyls: 80, heads: 2 },
    Preset { label: "PC 360K (5.25\")", format: "ibm.360", extension: "img", cyls: 40, heads: 2 },
    Preset { label: "C64 1541", format: "commodore.1541", extension: "d64", cyls: 40, heads: 1 },
    Preset { label: "C128 1571", format: "commodore.1571", extension: "d71", cyls: 40, heads: 2 },
    Preset { label: "Commodore 1581", format: "commodore.1581", extension: "d81", cyls: 80, heads: 2 },
    Preset { label: "Apple II DOS 3.3", format: "apple2.appledos.140", extension: "do", cyls: 35, heads: 1 },
    Preset { label: "Mac 800K", format: "mac.800", extension: "img", cyls: 80, heads: 2 },
    Preset { label: "BBC Micro DFS", format: "acorn.dfs.ss", extension: "ssd", cyls: 40, heads: 1 },
    Preset { label: "Amstrad CPC / +3 (scan)", format: "ibm.scan", extension: "dsk", cyls: 40, heads: 1 },
    Preset { label: "MSX 2DD", format: "msx.2dd", extension: "img", cyls: 80, heads: 2 },
    Preset { label: "Flux (SCP)", format: "", extension: "scp", cyls: 82, heads: 2 },
];

/// Guess a disk format from an image file's suffix and size.
pub fn guess_format(path: &Path, size: u64) -> Option<&'static str> {
    let ext = extension(path)?;
    let f = match (ext.as_str(), size) {
        ("adf", 901_120) => "amiga.amigados",
        ("adf", 1_802_240) => "amiga.amigados_hd",
        ("st", 368_640) => "atarist.360",
        ("st", 409_600) => "atarist.400",
        ("st", 450_560) => "atarist.440",
        ("st", 737_280) => "atarist.720",
        ("st", 819_200) => "atarist.800",
        ("st", 901_120) => "atarist.880",
        ("st" | "img" | "ima", 1_474_560) => "ibm.1440",
        ("img" | "ima", 163_840) => "ibm.160",
        ("img" | "ima", 184_320) => "ibm.180",
        ("img" | "ima", 327_680) => "ibm.320",
        ("img" | "ima", 368_640) => "ibm.360",
        ("img" | "ima", 737_280) => "ibm.720",
        ("img" | "ima", 819_200) => "ibm.800",
        ("img" | "ima", 1_228_800) => "ibm.1200",
        ("img" | "ima", 1_720_320) => "ibm.1680",
        ("img" | "ima", 2_949_120) => "ibm.2880",
        ("d64", 174_848 | 175_531 | 196_608 | 197_376) => "commodore.1541",
        ("d71", 349_696 | 351_062) => "commodore.1571",
        ("d81", 819_200) => "commodore.1581",
        ("do", 143_360) => "apple2.appledos.140",
        ("po", 143_360) => "apple2.prodos.140",
        _ => return None,
    };
    Some(f)
}

/// Expected image size in bytes for sector formats with a fixed layout.
pub fn expected_size(format: &str) -> Option<&'static [u64]> {
    Some(match format {
        "amiga.amigados" => &[901_120],
        "amiga.amigados_hd" => &[1_802_240],
        "ibm.1440" => &[1_474_560],
        "ibm.720" | "atarist.720" => &[737_280],
        "ibm.1200" => &[1_228_800],
        "ibm.360" | "atarist.360" => &[368_640],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_catalog_is_complete() {
        let c = Catalog::default();
        assert!(c.knows_format("amiga.amigados"));
        assert!(c.knows_format("zx.watford.ss40"));
        assert!(c.knows_suffix(Path::new("x.EDSK")));
        assert!(!c.knows_suffix(Path::new("x.zip")));
        assert!(c.formats.len() > 140);
    }

    #[test]
    fn parses_help_output() {
        let help = "usage: gw read\n\nFORMAT options:\namiga.amigados     ibm.1440\nzx.opus.ss40\n\nSupported file suffixes:\n.adf   .img\n.scp\n";
        let c = parse_help(help).unwrap();
        assert_eq!(c.formats, vec!["amiga.amigados", "ibm.1440", "zx.opus.ss40"]);
        assert_eq!(c.suffixes, vec!["adf", "img", "scp"]);
    }

    #[test]
    fn guesses_formats() {
        assert_eq!(guess_format(Path::new("a.ADF"), 901_120), Some("amiga.amigados"));
        assert_eq!(guess_format(Path::new("a.st"), 737_280), Some("atarist.720"));
        assert_eq!(guess_format(Path::new("a.img"), 737_280), Some("ibm.720"));
        assert_eq!(guess_format(Path::new("a.adf"), 12), None);
    }
}
