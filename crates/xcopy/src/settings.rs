// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Options of the main screen, remembered between sessions.

use hgf_core::gw::{Drive, GwConfig, TrackOpts};
use hgf_core::TrackSet;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    #[default]
    Copy,
    Tools,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyMode {
    /// AmigaDOS sector copy (ADF), standard retries.
    #[default]
    DosCopy,
    /// Sector copy with the recovery profile (more revolutions, retries and re-seeks).
    DosCopyPlus,
    /// Reads only the cylinders holding allocated blocks (bitmap of the disk), faster.
    BamCopy,
    /// Flux-level copy (SCP): keeps copy protections and non-DOS tracks.
    Nibble,
}

impl CopyMode {
    pub const ALL: [CopyMode; 4] =
        [CopyMode::DosCopy, CopyMode::DosCopyPlus, CopyMode::BamCopy, CopyMode::Nibble];
    pub fn label(self) -> &'static str {
        match self {
            CopyMode::DosCopy => "DOSCOPY",
            CopyMode::DosCopyPlus => "DOSCOPY+",
            CopyMode::BamCopy => "BAMCOPY",
            CopyMode::Nibble => "NIBBLE",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            CopyMode::DosCopy => "DOSCOPY: AMIGADOS SECTOR COPY, VERIFIED WRITE",
            CopyMode::DosCopyPlus => "DOSCOPY+: SECTOR COPY WITH RECOVERY (5 REVS, 8 RETRIES)",
            CopyMode::Nibble => "NIBBLE: FLUX COPY (SCP), KEEPS COPY PROTECTIONS",
            CopyMode::BamCopy => "BAMCOPY: READS ONLY THE TRACKS USED BY FILES (FASTER)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolMode {
    #[default]
    Format,
    QFormat,
    Erase,
    Speed,
    Clean,
    /// Erase and rewrite only cylinder 0 (boot block), with pre-erase and 8 verify retries.
    Track0,
    /// Amiga 1000 Kickstart disk from a 256K ROM image (like WriteKickDisk).
    KickDisk,
}

impl ToolMode {
    pub const ALL: [ToolMode; 7] = [
        ToolMode::Format,
        ToolMode::QFormat,
        ToolMode::Track0,
        ToolMode::KickDisk,
        ToolMode::Erase,
        ToolMode::Speed,
        ToolMode::Clean,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ToolMode::Format => "FORMAT",
            ToolMode::QFormat => "QFORMAT",
            ToolMode::Erase => "ERASE",
            ToolMode::Speed => "SPEEDCHK",
            ToolMode::Clean => "CLEAN",
            ToolMode::Track0 => "TRACK0",
            ToolMode::KickDisk => "KICKDISK",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            ToolMode::Format => "FORMAT: WRITE AN EMPTY AMIGADOS DISK, VERIFIED",
            ToolMode::QFormat => "QFORMAT: QUICK FORMAT, NO VERIFY PASS",
            ToolMode::Erase => "ERASE: DC-ERASE THE TRACK RANGE (UNFORMATTED DISK)",
            ToolMode::Speed => "SPEEDCHK: MEASURE THE SOURCE DRIVE RPM",
            ToolMode::Clean => "CLEAN: RUN A HEAD-CLEANING DISK IN THE TARGET DRIVE",
            ToolMode::Track0 => "TRACK0: ERASE + REWRITE TRACK 0 ONLY (BOOT BLOCK), 8 RETRIES",
            ToolMode::KickDisk => "KICKDISK: AMIGA 1000 KICKSTART DISK FROM A 256K ROM IMAGE",
        }
    }
    pub fn is_format(self) -> bool {
        matches!(self, ToolMode::Format | ToolMode::QFormat)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    #[default]
    Both,
    Upper,
    Lower,
}

impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Side::Both => "BOTH",
            Side::Upper => "UPPER",
            Side::Lower => "LOWER",
        }
    }
    /// Amiga convention: the lower head is side 0, the upper head side 1.
    pub fn heads(self) -> Vec<u8> {
        match self {
            Side::Both => vec![0, 1],
            Side::Upper => vec![1],
            Side::Lower => vec![0],
        }
    }
    pub fn next(self) -> Self {
        match self {
            Side::Both => Side::Upper,
            Side::Upper => Side::Lower,
            Side::Lower => Side::Both,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flow {
    #[default]
    DiskToDisk,
    DiskToAdf,
    AdfToDisk,
}

impl Flow {
    pub fn label(self) -> &'static str {
        match self {
            Flow::DiskToDisk => "DF>DF",
            Flow::DiskToAdf => "DF>ADF",
            Flow::AdfToDisk => "ADF>DF",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Flow::DiskToDisk => Flow::DiskToAdf,
            Flow::DiskToAdf => Flow::AdfToDisk,
            Flow::AdfToDisk => Flow::DiskToDisk,
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            Flow::DiskToDisk => "DEVICE DF>DF: DISK TO DISK (SAME DRIVE = SWAP DISKS)",
            Flow::DiskToAdf => "DEVICE DF>ADF: READ THE SOURCE DISK INTO AN IMAGE FILE",
            Flow::AdfToDisk => "DEVICE ADF>DF: WRITE AN IMAGE FILE TO THE TARGET DISK(S)",
        }
    }
}

/// How the drives are wired to the Greaseweazle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bus {
    /// PC drives on a (twisted) PC cable: DF0 = drive A, DF1 = drive B.
    #[default]
    Ibm,
    /// Shugart bus (Amiga-style drives / straight cable): DF0..DF3 = units 0..3.
    Shugart,
}

impl Bus {
    pub fn label(self) -> &'static str {
        match self {
            Bus::Ibm => "IBM PC (DF0=A DF1=B)",
            Bus::Shugart => "SHUGART (DF0-DF3 = 0-3)",
        }
    }
    pub fn drive(self, i: usize) -> Option<Drive> {
        match (self, i) {
            (Bus::Ibm, 0) => Some(Drive::A),
            (Bus::Ibm, 1) => Some(Drive::B),
            (Bus::Shugart, 0) => Some(Drive::Unit0),
            (Bus::Shugart, 1) => Some(Drive::Unit1),
            (Bus::Shugart, 2) => Some(Drive::Unit2),
            (Bus::Shugart, 3) => Some(Drive::Unit3),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub cfg: GwConfig,
    pub bus: Bus,
    pub section: Section,
    pub copy_mode: CopyMode,
    pub tool_mode: ToolMode,
    pub start: u16,
    pub end: u16,
    pub side: Side,
    pub hd: bool,
    pub flow: Flow,
    /// Tools / FORMAT: save the blank disk as an ADF instead of writing it.
    pub tool_adf: bool,
    pub source: usize,
    pub targets: [bool; 4],
    pub volume: String,
    pub ffs: bool,
    pub last_dir: Option<PathBuf>,
    /// Jingles at the end of an operation.
    pub sound: bool,
}

pub const MAX_CYL: u16 = 83;

impl Default for Settings {
    fn default() -> Self {
        Self {
            cfg: GwConfig::default(),
            bus: Bus::Ibm,
            section: Section::Copy,
            copy_mode: CopyMode::DosCopy,
            tool_mode: ToolMode::Format,
            start: 0,
            end: 79,
            side: Side::Both,
            hd: false,
            flow: Flow::DiskToDisk,
            tool_adf: false,
            source: 0,
            targets: [true, false, false, false],
            volume: "Empty".into(),
            ffs: false,
            last_dir: None,
            sound: true,
        }
    }
}

impl Settings {
    /// Reset the copy options (DEFAULT button), keeping the hardware setup.
    pub fn defaults(&mut self) {
        let keep = (self.cfg.clone(), self.bus, self.last_dir.clone(), self.volume.clone(), self.sound);
        *self = Settings::default();
        (self.cfg, self.bus, self.last_dir, self.volume, self.sound) = keep;
    }

    pub fn sector_format(&self) -> &'static str {
        if self.hd {
            "amiga.amigados_hd"
        } else {
            "amiga.amigados"
        }
    }

    pub fn full_range(&self) -> bool {
        self.start == 0 && self.end == 79 && self.side == Side::Both
    }

    /// Track selection for gw and the provisional map geometry.
    pub fn tracks(&self) -> (TrackOpts, TrackSet) {
        let heads = self.side.heads();
        let geo = TrackSet { cyls: (self.start..=self.end).collect(), heads: heads.clone() };
        let mut t = TrackOpts::default();
        if !self.full_range() {
            let h = if heads.len() == 2 { "0-1".to_string() } else { heads[0].to_string() };
            t.custom = format!("c={}-{}:h={h}", self.start, self.end);
        }
        (t, geo)
    }

    pub fn start_head(&self) -> u8 {
        if self.side == Side::Upper {
            1
        } else {
            0
        }
    }

    pub fn end_head(&self) -> u8 {
        if self.side == Side::Lower {
            0
        } else {
            1
        }
    }

    pub fn target_list(&self) -> Vec<usize> {
        (0..4).filter(|&i| self.targets[i] && self.bus.drive(i).is_some()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_spec() {
        let mut s = Settings::default();
        assert_eq!(s.tracks().0.spec(), None);
        s.start = 10;
        s.end = 20;
        s.side = Side::Upper;
        let (t, g) = s.tracks();
        assert_eq!(t.spec().as_deref(), Some("c=10-20:h=1"));
        assert_eq!(g.len(), 11);
        assert!(hgf_core::disk::trackset::validate_tspec(&t.spec().unwrap()).is_ok());
    }

    #[test]
    fn ibm_bus_has_two_drives() {
        assert!(Bus::Ibm.drive(2).is_none());
        assert_eq!(Bus::Shugart.drive(3), Some(Drive::Unit3));
    }
}
