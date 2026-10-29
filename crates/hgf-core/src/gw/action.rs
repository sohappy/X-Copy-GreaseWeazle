// SPDX-License-Identifier: GPL-3.0-or-later

//! Model of every `gw` action with its options, argument generation and pre-flight validation.

use crate::disk::formats::{self, Catalog};
use crate::disk::imageopts;
use crate::disk::trackset::validate_tspec;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt::Display;
use std::path::{Path, PathBuf};

/// Drive (and bus) identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Drive {
    #[default]
    A,
    B,
    Unit0,
    Unit1,
    Unit2,
    Unit3,
}

impl Drive {
    pub const ALL: [Drive; 6] = [Drive::A, Drive::B, Drive::Unit0, Drive::Unit1, Drive::Unit2, Drive::Unit3];

    pub fn arg(self) -> &'static str {
        match self {
            Drive::A => "A",
            Drive::B => "B",
            Drive::Unit0 => "0",
            Drive::Unit1 => "1",
            Drive::Unit2 => "2",
            Drive::Unit3 => "3",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Drive::A => "Drive A (PC bus)",
            Drive::B => "Drive B (PC bus)",
            Drive::Unit0 => "Unit 0 (Shugart)",
            Drive::Unit1 => "Unit 1 (Shugart)",
            Drive::Unit2 => "Unit 2 (Shugart)",
            Drive::Unit3 => "Unit 3 (Shugart)",
        }
    }
}

/// Density select level on pin 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Densel {
    #[default]
    Auto,
    High,
    Low,
}

impl Densel {
    fn arg(self) -> Option<&'static str> {
        match self {
            Densel::Auto => None,
            Densel::High => Some("H"),
            Densel::Low => Some("L"),
        }
    }
}

/// Global settings applied to every invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GwConfig {
    /// Path to the `gw` executable (or just `gw` when on the PATH).
    pub program: String,
    /// `--device` (serial port); empty = auto-detect.
    pub device: String,
    /// `--diskdefs` custom disk definitions file; empty = built-in.
    pub diskdefs: String,
    /// Global `--time` flag.
    pub time: bool,
    /// Global `--bt` flag: Python backtrace on errors, for bug reports.
    pub backtrace: bool,
}

impl Default for GwConfig {
    fn default() -> Self {
        Self {
            program: "gw".into(),
            device: String::new(),
            diskdefs: String::new(),
            time: false,
            backtrace: false,
        }
    }
}

/// Track selection helper shared by read/write/convert/erase.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackOpts {
    /// Free TSPEC; empty = format default. Never remembered between sessions: a forgotten
    /// diagnostic setting (e.g. a head offset) would silently restrict every later read.
    #[serde(skip)]
    pub custom: String,
    /// `step=2`: 40-track media in an 80-track drive.
    pub double_step: bool,
    /// `hswap`: swap the physical heads.
    pub hswap: bool,
}

impl TrackOpts {
    /// TSPEC to pass as `--tracks`, or `None` to keep the format's own geometry.
    ///
    /// gw merges `--tracks` into the disk definition, so `step=2` alone is valid.
    pub fn spec(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        let custom = self.custom.trim();
        if !custom.is_empty() {
            parts.push(custom.to_string());
        }
        if self.double_step && !custom.contains("step=") {
            parts.push("step=2".into());
        }
        if self.hswap && !custom.contains("hswap") {
            parts.push("hswap".into());
        }
        (!parts.is_empty()).then(|| parts.join(":"))
    }

    fn validate(&self) -> Result<(), String> {
        match self.spec() {
            Some(s) => validate_tspec(&s).map_err(|e| format!("Tracks: {e}")),
            None => Ok(()),
        }
    }
}

/// Low-level flux options shared by the reading commands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FluxOpts {
    pub fake_index: String,
    pub hard_sectors: bool,
    pub adjust_speed: String,
    pub pll: String,
    pub reverse: bool,
}

/// Read profiles replacing the old "Diagnostic"/"Recovery" buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadProfile {
    /// gw defaults (3 retries).
    #[default]
    Standard,
    /// More revolutions, retries and seek-retries for damaged disks.
    Recovery,
    /// No retries; map the disk quickly, the image is discarded.
    QuickScan,
}

impl ReadProfile {
    pub fn label(self) -> &'static str {
        match self {
            ReadProfile::Standard => "Standard",
            ReadProfile::Recovery => "Recovery",
            ReadProfile::QuickScan => "Quick scan",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            ReadProfile::Standard => "gw defaults: 3 retries per track.",
            ReadProfile::Recovery => {
                "Damaged disks: 5 revolutions, 8 retries and 2 head re-seeks per track. \
                 The recovered image is saved to your output file."
            }
            ReadProfile::QuickScan => {
                "Health check: one pass without retries. The image goes to a temporary \
                 file that is deleted afterwards."
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadOpts {
    pub output: PathBuf,
    pub format: String,
    pub profile: ReadProfile,
    pub revs: Option<u32>,
    pub retries: Option<u32>,
    pub seek_retries: Option<u32>,
    pub tracks: TrackOpts,
    pub raw: bool,
    pub flux: FluxOpts,
    pub densel: Densel,
    pub gen_tg43: bool,
    #[serde(skip)]
    pub no_clobber: bool,
    /// Image file options (`file.hfe::bitrate=250`), see [`crate::disk::imageopts`].
    pub file_opts: String,
}

impl ReadOpts {
    /// Apply the profile: returns the effective options.
    pub fn effective(&self) -> ReadOpts {
        let mut o = self.clone();
        match self.profile {
            ReadProfile::Standard => {}
            ReadProfile::Recovery => {
                o.revs = Some(o.revs.unwrap_or(5).max(5));
                o.retries = Some(o.retries.unwrap_or(8).max(8));
                o.seek_retries = Some(o.seek_retries.unwrap_or(2).max(2));
            }
            ReadProfile::QuickScan => {
                o.retries = Some(0);
                o.seek_retries = Some(0);
            }
        }
        o
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WriteOpts {
    pub input: PathBuf,
    /// Image file options of the source (e.g. D88 `index`).
    pub file_opts: String,
    pub format: String,
    pub tracks: TrackOpts,
    pub pre_erase: bool,
    pub erase_empty: bool,
    pub no_verify: bool,
    pub retries: Option<u32>,
    pub precomp: String,
    pub fake_index: String,
    pub hard_sectors: bool,
    pub reverse: bool,
    pub densel: Densel,
    pub gen_tg43: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConvertOpts {
    pub input: PathBuf,
    pub output: PathBuf,
    pub in_opts: String,
    pub out_opts: String,
    pub format: String,
    pub tracks: TrackOpts,
    pub out_tracks: String,
    pub adjust_speed: String,
    pub pll: String,
    pub hard_sectors: bool,
    pub reverse: bool,
    #[serde(skip)]
    pub no_clobber: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EraseOpts {
    pub revs: Option<u32>,
    pub tracks: TrackOpts,
    pub hfreq: bool,
    pub fake_index: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanOpts {
    pub cyls: u32,
    pub passes: u32,
    pub linger: u32,
}

impl Default for CleanOpts {
    fn default() -> Self {
        Self { cyls: 80, passes: 3, linger: 100 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeekOpts {
    pub cylinder: u32,
    pub motor_on: bool,
}

impl SeekOpts {
    /// Cylinders gw considers normal; others need an explicit confirmation.
    pub fn is_extreme(&self) -> bool {
        self.cylinder > 83
    }
}

/// `gw delays`: `None` fields are left unchanged (display only when all are `None`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelaysOpts {
    pub select: Option<u32>,
    pub step: Option<u32>,
    pub settle: Option<u32>,
    pub motor: Option<u32>,
    pub watchdog: Option<u32>,
    pub pre_write: Option<u32>,
    pub post_write: Option<u32>,
    pub index_mask: Option<u32>,
}

impl DelaysOpts {
    pub fn is_query(&self) -> bool {
        *self == DelaysOpts::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateOpts {
    pub file: PathBuf,
    pub tag: String,
    /// Risky flags are never remembered between sessions.
    #[serde(skip)]
    pub force: bool,
    #[serde(skip)]
    pub bootloader: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlignOpts {
    pub format: String,
    pub tracks: String,
    pub revs: Option<u32>,
    pub reads: Option<u32>,
    pub raw: bool,
    pub flux: FluxOpts,
    pub densel: Densel,
    pub gen_tg43: bool,
}

impl Default for AlignOpts {
    fn default() -> Self {
        Self {
            format: String::new(),
            tracks: "c=0:h=0".into(),
            revs: None,
            reads: Some(100),
            raw: false,
            flux: FluxOpts::default(),
            densel: Densel::Auto,
            gen_tg43: false,
        }
    }
}

/// Every Greaseweazle action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Info { bootloader: bool },
    Read(ReadOpts),
    Write(WriteOpts),
    Convert(ConvertOpts),
    Erase(EraseOpts),
    Clean(CleanOpts),
    Seek(SeekOpts),
    Delays(DelaysOpts),
    Update(UpdateOpts),
    PinGet { pin: u32 },
    PinSet { pin: u32, high: bool },
    Reset { delays: bool },
    Bandwidth,
    Rpm { nr: u32 },
    Align(AlignOpts),
}

/// Discriminant of [`Action`], convenient for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Info,
    Read,
    Write,
    Convert,
    Erase,
    Clean,
    Seek,
    Delays,
    Update,
    PinGet,
    PinSet,
    Reset,
    Bandwidth,
    Rpm,
    Align,
}

impl ActionKind {
    /// Progressive verb shown in the status badge.
    pub fn verb(self) -> &'static str {
        match self {
            ActionKind::Info => "PROBING",
            ActionKind::Read => "READING",
            ActionKind::Write => "WRITING",
            ActionKind::Convert => "CONVERTING",
            ActionKind::Erase => "ERASING",
            ActionKind::Clean => "CLEANING",
            ActionKind::Seek => "SEEKING",
            ActionKind::Delays => "DELAYS",
            ActionKind::Update => "UPDATING",
            ActionKind::PinGet | ActionKind::PinSet => "PIN",
            ActionKind::Reset => "RESETTING",
            ActionKind::Bandwidth => "BANDWIDTH",
            ActionKind::Rpm => "RPM",
            ActionKind::Align => "ALIGNING",
        }
    }

    /// Whether the disk visualizer follows this action.
    pub fn uses_disk_map(self) -> bool {
        matches!(self, ActionKind::Read | ActionKind::Write | ActionKind::Convert | ActionKind::Erase)
    }
}

struct Args(Vec<OsString>);

impl Args {
    fn new(action: &str) -> Self {
        Self(vec![action.into()])
    }
    fn push(&mut self, s: impl Into<OsString>) {
        self.0.push(s.into());
    }
    fn flag(&mut self, name: &str, on: bool) {
        if on {
            self.push(name);
        }
    }
    fn text(&mut self, name: &str, value: &str) {
        let v = value.trim();
        if !v.is_empty() {
            self.push(name);
            self.push(v);
        }
    }
    fn num<T: Display>(&mut self, name: &str, value: Option<T>) {
        if let Some(v) = value {
            self.push(name);
            self.push(v.to_string());
        }
    }
    fn device(&mut self, cfg: &GwConfig) {
        self.text("--device", &cfg.device);
    }
    fn drive(&mut self, drive: Drive) {
        self.push("--drive");
        self.push(drive.arg());
    }
    fn diskdefs(&mut self, cfg: &GwConfig) {
        self.text("--diskdefs", &cfg.diskdefs);
    }
    fn densel(&mut self, d: Densel, tg43: bool) {
        if let Some(l) = d.arg() {
            self.text("--densel", l);
        }
        self.flag("--gen-tg43", tg43);
    }
    fn tracks(&mut self, t: &TrackOpts) {
        if let Some(s) = t.spec() {
            self.text("--tracks", &s);
        }
    }
    fn flux(&mut self, f: &FluxOpts, with_pll_speed: bool) {
        self.text("--fake-index", &f.fake_index);
        self.flag("--hard-sectors", f.hard_sectors);
        if with_pll_speed {
            self.text("--adjust-speed", &f.adjust_speed);
            self.text("--pll", &f.pll);
        }
        self.flag("--reverse", f.reverse);
    }
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        match self {
            Action::Info { .. } => ActionKind::Info,
            Action::Read(_) => ActionKind::Read,
            Action::Write(_) => ActionKind::Write,
            Action::Convert(_) => ActionKind::Convert,
            Action::Erase(_) => ActionKind::Erase,
            Action::Clean(_) => ActionKind::Clean,
            Action::Seek(_) => ActionKind::Seek,
            Action::Delays(_) => ActionKind::Delays,
            Action::Update(_) => ActionKind::Update,
            Action::PinGet { .. } => ActionKind::PinGet,
            Action::PinSet { .. } => ActionKind::PinSet,
            Action::Reset { .. } => ActionKind::Reset,
            Action::Bandwidth => ActionKind::Bandwidth,
            Action::Rpm { .. } => ActionKind::Rpm,
            Action::Align(_) => ActionKind::Align,
        }
    }

    /// Full argument list (without the program name).
    pub fn args(&self, cfg: &GwConfig, drive: Drive) -> Vec<OsString> {
        let mut a = match self {
            Action::Info { bootloader } => {
                let mut a = Args::new("info");
                a.device(cfg);
                a.flag("--bootloader", *bootloader);
                a
            }
            Action::Read(o) => {
                let o = o.effective();
                let mut a = Args::new("read");
                a.device(cfg);
                a.drive(drive);
                a.diskdefs(cfg);
                a.text("--format", &o.format);
                a.num("--revs", o.revs);
                a.tracks(&o.tracks);
                a.flag("--raw", o.raw);
                a.flux(&o.flux, true);
                a.num("--retries", o.retries);
                a.num("--seek-retries", o.seek_retries);
                a.flag("--no-clobber", o.no_clobber);
                a.densel(o.densel, o.gen_tg43);
                a.push(imageopts::with_opts(&o.output, &o.file_opts));
                a
            }
            Action::Write(o) => {
                let mut a = Args::new("write");
                a.device(cfg);
                a.drive(drive);
                a.diskdefs(cfg);
                a.text("--format", &o.format);
                a.tracks(&o.tracks);
                a.flag("--pre-erase", o.pre_erase);
                a.flag("--erase-empty", o.erase_empty);
                a.text("--fake-index", &o.fake_index);
                a.flag("--hard-sectors", o.hard_sectors);
                a.flag("--no-verify", o.no_verify);
                a.num("--retries", o.retries);
                a.text("--precomp", &o.precomp);
                a.flag("--reverse", o.reverse);
                a.densel(o.densel, o.gen_tg43);
                a.push(imageopts::with_opts(&o.input, &o.file_opts));
                a
            }
            Action::Convert(o) => {
                let mut a = Args::new("convert");
                a.diskdefs(cfg);
                a.text("--format", &o.format);
                a.tracks(&o.tracks);
                a.text("--out-tracks", &o.out_tracks);
                a.text("--adjust-speed", &o.adjust_speed);
                a.flag("--no-clobber", o.no_clobber);
                a.text("--pll", &o.pll);
                a.flag("--hard-sectors", o.hard_sectors);
                a.flag("--reverse", o.reverse);
                a.push(imageopts::with_opts(&o.input, &o.in_opts));
                a.push(imageopts::with_opts(&o.output, &o.out_opts));
                a
            }
            Action::Erase(o) => {
                let mut a = Args::new("erase");
                a.device(cfg);
                a.drive(drive);
                a.num("--revs", o.revs);
                a.tracks(&o.tracks);
                a.flag("--hfreq", o.hfreq);
                a.text("--fake-index", &o.fake_index);
                a
            }
            Action::Clean(o) => {
                let mut a = Args::new("clean");
                a.device(cfg);
                a.drive(drive);
                a.num("--cyls", Some(o.cyls));
                a.num("--passes", Some(o.passes));
                a.num("--linger", Some(o.linger));
                a
            }
            Action::Seek(o) => {
                let mut a = Args::new("seek");
                a.device(cfg);
                a.drive(drive);
                // gw would prompt on stdin for extreme cylinders; the GUI confirms instead.
                a.flag("--force", true);
                a.flag("--motor-on", o.motor_on);
                a.push(o.cylinder.to_string());
                a
            }
            Action::Delays(o) => {
                let mut a = Args::new("delays");
                a.device(cfg);
                a.num("--select", o.select);
                a.num("--step", o.step);
                a.num("--settle", o.settle);
                a.num("--motor", o.motor);
                a.num("--watchdog", o.watchdog);
                a.num("--pre-write", o.pre_write);
                a.num("--post-write", o.post_write);
                a.num("--index-mask", o.index_mask);
                a
            }
            Action::Update(o) => {
                let mut a = Args::new("update");
                if !o.file.as_os_str().is_empty() {
                    a.push("--file");
                    a.push(o.file.as_os_str());
                }
                a.text("--tag", &o.tag);
                a.device(cfg);
                a.flag("--force", o.force);
                a.flag("--bootloader", o.bootloader);
                a
            }
            Action::PinGet { pin } => {
                let mut a = Args::new("pin");
                a.push("get");
                a.device(cfg);
                a.drive(drive);
                a.push(pin.to_string());
                a
            }
            Action::PinSet { pin, high } => {
                let mut a = Args::new("pin");
                a.push("set");
                a.device(cfg);
                a.push(pin.to_string());
                a.push(if *high { "H" } else { "L" });
                a
            }
            Action::Reset { delays } => {
                let mut a = Args::new("reset");
                a.device(cfg);
                a.flag("--delays", *delays);
                a
            }
            Action::Bandwidth => {
                let mut a = Args::new("bandwidth");
                a.device(cfg);
                a
            }
            Action::Rpm { nr } => {
                let mut a = Args::new("rpm");
                a.device(cfg);
                a.drive(drive);
                a.num("--nr", Some(*nr));
                a
            }
            Action::Align(o) => {
                let mut a = Args::new("align");
                a.device(cfg);
                a.drive(drive);
                a.diskdefs(cfg);
                a.text("--format", &o.format);
                a.num("--revs", o.revs);
                a.text("--tracks", &o.tracks);
                a.num("--reads", o.reads);
                a.flag("--raw", o.raw);
                a.flux(&o.flux, true);
                a.densel(o.densel, o.gen_tg43);
                a
            }
        };
        if cfg.time {
            a.0.insert(0, "--time".into());
        }
        if cfg.backtrace {
            a.0.insert(0, "--bt".into());
        }
        a.0
    }

    /// Shell-like rendering of the command, for the console.
    pub fn command_line(&self, cfg: &GwConfig, drive: Drive) -> String {
        let mut s = cfg.program.clone();
        for a in self.args(cfg, drive) {
            let a = a.to_string_lossy();
            s.push(' ');
            if a.is_empty() || a.contains(char::is_whitespace) {
                s.push_str(&format!("\"{a}\""));
            } else {
                s.push_str(&a);
            }
        }
        s
    }

    /// File the action creates, used for overwrite confirmation.
    pub fn output_path(&self) -> Option<&Path> {
        match self {
            Action::Read(o) if o.profile != ReadProfile::QuickScan => Some(&o.output),
            Action::Convert(o) => Some(&o.output),
            _ => None,
        }
    }

    /// Mark the output as allowed (or not) to be overwritten.
    pub fn set_no_clobber(&mut self, on: bool) {
        match self {
            Action::Read(o) => o.no_clobber = on,
            Action::Convert(o) => o.no_clobber = on,
            _ => {}
        }
    }

    /// Description of the irreversible effect of this action, if any.
    pub fn danger(&self) -> Option<String> {
        match self {
            Action::Write(o) => Some(format!(
                "The floppy disk will be overwritten with '{}'.",
                file_name(&o.input)
            )),
            Action::Erase(_) => Some("All magnetic data on the selected tracks will be destroyed.".into()),
            Action::Clean(_) => Some("Only run this with a CLEANING disk in the drive: it scrubs the head over the whole surface.".into()),
            Action::Update(o) if o.bootloader => Some("Updating the BOOTLOADER can brick the device if interrupted. Proceed only if you know you need it.".into()),
            Action::Update(_) => Some("The device firmware will be replaced. Do not unplug the Greaseweazle during the update.".into()),
            Action::Delays(o) if !o.is_query() => Some("Drive timing parameters will be changed on the device.".into()),
            Action::Seek(o) if o.is_extreme() => Some(format!(
                "Cylinder {} is beyond the usual range (0-83) and may hit the drive's end stop.",
                o.cylinder
            )),
            Action::PinSet { pin, high } => Some(format!(
                "Pin {pin} of the drive interface will be set {}.",
                if *high { "High" } else { "Low" }
            )),
            _ => None,
        }
    }

    /// Pre-flight checks. `Ok` carries non-blocking warnings.
    pub fn validate(&self, catalog: &Catalog) -> Result<Vec<String>, String> {
        let mut warnings = Vec::new();
        let check_format = |f: &str| -> Result<(), String> {
            if !f.is_empty() && !catalog.knows_format(f) {
                return Err(format!("Unknown disk format '{f}'."));
            }
            Ok(())
        };
        match self {
            Action::Read(o) => {
                check_format(&o.format)?;
                o.tracks.validate()?;
                if o.profile != ReadProfile::QuickScan {
                    check_output(&o.output, catalog)?;
                }
                let ext = formats::extension(&o.output).unwrap_or_default();
                if o.format.is_empty() && formats::NEEDS_FORMAT_SUFFIXES.contains(&ext.as_str()) {
                    return Err(format!("A disk format is required to create a .{ext} image."));
                }
                if o.raw && !formats::FLUX_SUFFIXES.contains(&ext.as_str()) {
                    warnings.push("--raw only makes sense with a flux image (.scp, .raw…).".into());
                }
                imageopts::validate(imageopts::for_output(&o.output), &o.output, &o.file_opts)?;
                if ext == "hfe" && o.format.is_empty() && imageopts::get(&o.file_opts, "bitrate").is_none() {
                    warnings
                        .push("HFE without a disk format: set the bitrate (250 for DD, 500 for HD).".into());
                }
            }
            Action::Write(o) => {
                check_format(&o.format)?;
                o.tracks.validate()?;
                check_input(&o.input, catalog)?;
                let ext = formats::extension(&o.input).unwrap_or_default();
                let size = std::fs::metadata(&o.input).map(|m| m.len()).unwrap_or(0);
                if o.format.is_empty() && formats::NEEDS_FORMAT_SUFFIXES.contains(&ext.as_str()) {
                    return Err(format!("Select the disk format of this .{ext} image."));
                }
                if let Some(sizes) = formats::expected_size(&o.format) {
                    let sector_image = !formats::FLUX_SUFFIXES.contains(&ext.as_str())
                        && ["adf", "img", "ima", "st"].contains(&ext.as_str());
                    if sector_image && !sizes.contains(&size) {
                        return Err(format!(
                            "Image size {size} bytes does not match format {} (expected {}).",
                            o.format,
                            sizes.iter().map(u64::to_string).collect::<Vec<_>>().join(" or ")
                        ));
                    }
                }
                if o.no_verify {
                    warnings.push("Verification is disabled: write errors will go unnoticed.".into());
                }
                imageopts::validate(imageopts::for_input(&o.input), &o.input, &o.file_opts)?;
            }
            Action::Convert(o) => {
                check_format(&o.format)?;
                o.tracks.validate()?;
                if !o.out_tracks.trim().is_empty() {
                    validate_tspec(&o.out_tracks).map_err(|e| format!("Output tracks: {e}"))?;
                }
                check_input(&o.input, catalog)?;
                check_output(&o.output, catalog)?;
                if same_file(&o.input, &o.output) {
                    return Err("Input and output must be different files.".into());
                }
                imageopts::validate(imageopts::for_input(&o.input), &o.input, &o.in_opts)?;
                imageopts::validate(imageopts::for_output(&o.output), &o.output, &o.out_opts)?;
                let ext = formats::extension(&o.output).unwrap_or_default();
                if ext == "hfe" && o.format.is_empty() && imageopts::get(&o.out_opts, "bitrate").is_none() {
                    warnings
                        .push("HFE without a disk format: set the bitrate (250 for DD, 500 for HD).".into());
                }
            }
            Action::Erase(o) => o.tracks.validate()?,
            Action::Align(o) => {
                check_format(&o.format)?;
                validate_tspec(&o.tracks).map_err(|e| format!("Tracks: {e}"))?;
            }
            Action::Update(o) if !o.file.as_os_str().is_empty() && !o.file.is_file() => {
                return Err(format!("Firmware file not found: {}", o.file.display()));
            }
            Action::Clean(o) if o.cyls == 0 || o.passes == 0 => {
                return Err("Cylinders and passes must be at least 1.".into());
            }
            _ => {}
        }
        Ok(warnings)
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn check_input(p: &Path, catalog: &Catalog) -> Result<(), String> {
    if p.as_os_str().is_empty() {
        return Err("Choose an input image.".into());
    }
    if !p.is_file() {
        return Err(format!("Image not found: {}", p.display()));
    }
    if !catalog.knows_suffix(p) {
        return Err(format!("Unsupported image type: '{}'.", file_name(p)));
    }
    Ok(())
}

fn check_output(p: &Path, catalog: &Catalog) -> Result<(), String> {
    if p.as_os_str().is_empty() {
        return Err("Choose an output image file.".into());
    }
    if !catalog.knows_suffix(p) {
        return Err(format!(
            "Unsupported image type: '{}'. Pick a known extension (.adf, .scp, .img…).",
            file_name(p)
        ));
    }
    if let Some(dir) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
        if !dir.is_dir() {
            return Err(format!("Folder does not exist: {}", dir.display()));
        }
    }
    if p.is_dir() {
        return Err("The output path is a folder.".into());
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: Vec<OsString>) -> Vec<String> {
        v.into_iter().map(|x| x.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn read_keeps_format_geometry() {
        let o = ReadOpts { output: "disk.adf".into(), format: "amiga.amigados".into(), ..Default::default() };
        let a = s(Action::Read(o).args(&GwConfig::default(), Drive::A));
        assert_eq!(a, ["read", "--drive", "A", "--format", "amiga.amigados", "disk.adf"]);
    }

    #[test]
    fn read_profiles_and_double_step() {
        let o = ReadOpts {
            output: "c64.d64".into(),
            format: "commodore.1541".into(),
            profile: ReadProfile::Recovery,
            tracks: TrackOpts { double_step: true, ..Default::default() },
            ..Default::default()
        };
        let cfg = GwConfig { device: "/dev/ttyACM0".into(), ..Default::default() };
        let a = s(Action::Read(o).args(&cfg, Drive::Unit1));
        assert_eq!(
            a,
            [
                "read",
                "--device",
                "/dev/ttyACM0",
                "--drive",
                "1",
                "--format",
                "commodore.1541",
                "--revs",
                "5",
                "--tracks",
                "step=2",
                "--retries",
                "8",
                "--seek-retries",
                "2",
                "c64.d64"
            ]
        );
    }

    #[test]
    fn custom_tracks_are_not_remembered() {
        let t = TrackOpts { custom: "c=2-10:h0.off=+1".into(), double_step: true, hswap: false };
        let back: TrackOpts = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back.custom, "", "a forgotten diagnostic offset must not come back");
        assert!(back.double_step, "machine settings such as double step are kept");
    }

    #[test]
    fn seek_never_prompts() {
        let a =
            s(Action::Seek(SeekOpts { cylinder: 90, motor_on: true }).args(&GwConfig::default(), Drive::B));
        assert_eq!(a, ["seek", "--drive", "B", "--force", "--motor-on", "90"]);
    }

    #[test]
    fn global_time_flag_goes_first() {
        let cfg = GwConfig { time: true, ..Default::default() };
        let a = s(Action::Bandwidth.args(&cfg, Drive::A));
        assert_eq!(a, ["--time", "bandwidth"]);
    }

    #[test]
    fn backtrace_flag_and_image_options() {
        let cfg = GwConfig { time: true, backtrace: true, ..Default::default() };
        assert_eq!(s(Action::Bandwidth.args(&cfg, Drive::A)), ["--bt", "--time", "bandwidth"]);
        let r = ReadOpts {
            output: "disk.hfe".into(),
            format: "amiga.amigados".into(),
            file_opts: "bitrate=250:version=3".into(),
            ..Default::default()
        };
        let a = s(Action::Read(r).args(&GwConfig::default(), Drive::A));
        assert_eq!(a.last().unwrap(), "disk.hfe::bitrate=250:version=3");
        let c = ConvertOpts {
            input: "multi.d88".into(),
            output: "out.hfe".into(),
            in_opts: "index=1".into(),
            out_opts: "bitrate=500".into(),
            ..Default::default()
        };
        let a = s(Action::Convert(c).args(&GwConfig::default(), Drive::A));
        assert_eq!(&a[a.len() - 2..], ["multi.d88::index=1", "out.hfe::bitrate=500"]);
    }

    #[test]
    fn pin_set_has_no_drive() {
        let a = s(Action::PinSet { pin: 2, high: false }.args(&GwConfig::default(), Drive::A));
        assert_eq!(a, ["pin", "set", "2", "L"]);
    }

    #[test]
    fn validation_catches_common_mistakes() {
        let cat = Catalog::default();
        let read_img = Action::Read(ReadOpts { output: "x.img".into(), ..Default::default() });
        assert!(read_img.validate(&cat).unwrap_err().contains("format"));
        let bad_ext = Action::Read(ReadOpts { output: "x.zip".into(), ..Default::default() });
        assert!(bad_ext.validate(&cat).is_err());
        let missing = Action::Write(WriteOpts { input: "/nonexistent/x.adf".into(), ..Default::default() });
        assert!(missing.validate(&cat).is_err());
        let bad_format = Action::Read(ReadOpts {
            output: "x.adf".into(),
            format: "amiga.nope".into(),
            ..Default::default()
        });
        assert!(bad_format.validate(&cat).is_err());
    }

    #[test]
    fn write_checks_adf_size() {
        let dir = std::env::temp_dir().join(format!("hgf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("small.adf");
        std::fs::write(&p, [0u8; 1000]).unwrap();
        let w = Action::Write(WriteOpts {
            input: p.clone(),
            format: "amiga.amigados".into(),
            ..Default::default()
        });
        assert!(w.validate(&Catalog::default()).unwrap_err().contains("size"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
