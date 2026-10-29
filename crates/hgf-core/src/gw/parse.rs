// SPDX-License-Identifier: GPL-3.0-or-later

//! Understanding `gw` output.
//!
//! Every pattern here comes from the Greaseweazle host tools sources (`tools/*.py`); the
//! test fixtures in `tests/fixtures` are real captures.

use crate::disk::trackset::TrackSet;
use regex::Regex;
use std::sync::LazyLock;

static TRACK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^T(\d+)\.(\d+)(?: <- Drive \d+\.\d+)?: (.*)$").unwrap());
static SECTORS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\((\d+)/(\d+) sectors\)").unwrap());
static MISSING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Giving up: (\d+) sectors missing").unwrap());
static VERIFY_RETRY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Verify Failure: Retry #(\d+)").unwrap());
static GEOMETRY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(Reading|Writing|Erasing|Converting) (c=[^\s,]+)").unwrap());
static VERIFY_FAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Failed to verify Track (\d+)\.(\d+)").unwrap());
static RPM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(FASTEST|Ar\.Mean|Median|SLOWEST):\s+)?Rate: ([\d.]+) rpm ; Period: ([\d.]+) ms")
        .unwrap()
});
static BANDWIDTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Write|Read) Bandwidth:\s+([\d.]+) / +([\d.]+) / +([\d.]+) Mbps").unwrap()
});
static KV_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([A-Za-z][A-Za-z .\-]{0,40}?):\s+(\S.*)$").unwrap());
static MAP_ROW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d)\.\s*(\d+): (.*)$").unwrap());
static FOUND_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Found (\d+) sectors of (\d+)").unwrap());
static PIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Pin (\d+) is (?:set )?(\S+)").unwrap());
static FIRMWARE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"New firmware version (\S+) is available").unwrap());
static ELAPSED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Time elapsed: ([\d.]+) seconds").unwrap());

/// What `gw` said about one track.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackOutcome {
    /// Sectors decoded; `retry` is true when the line carries a `(Retry #s.r)` suffix.
    Decoded {
        ok: u32,
        total: u32,
        retry: bool,
    },
    /// gw gave up on the track.
    GaveUp {
        missing: u32,
    },
    /// Raw flux only (no format decoding).
    Flux,
    /// Track outside the format's definition.
    OutOfRange,
    /// Decoder found an empty track (`ibm.scan`).
    Empty,
    /// Write in progress; `Some(n)` when re-writing after a verify failure.
    Writing {
        verify_retry: Option<u32>,
    },
    Erasing,
    Other,
}

/// One interpreted output line.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Track {
        cyl: u16,
        head: u8,
        outcome: TrackOutcome,
    },
    /// Tracks covered by the operation (`Reading c=0-79:h=0-1 revs=3`).
    Geometry(TrackSet),
    /// `Command Failed: …` (gw still exits with status 0!).
    CommandFailed(String),
    /// `** FATAL ERROR:`; the following line carries the message.
    FatalHeader,
    /// `gw update` declined to update (`** SKIPPING UPDATE:` + reason).
    UpdateSkipped(String),
    VerifyFailed {
        cyl: u16,
        head: u8,
    },
    AllVerified,
    /// `Name: value` pairs from `info`, `delays`, …
    KeyValue {
        key: String,
        value: String,
    },
    /// `gw info` found no device.
    DeviceNotFound,
    FirmwareUpdate(String),
    Rpm {
        rpm: f64,
        ms: f64,
    },
    RpmStat {
        label: String,
        rpm: f64,
    },
    Bandwidth {
        direction: String,
        min: f64,
        mean: f64,
        max: f64,
    },
    /// Cylinders of the end-of-read sector map columns.
    SectorMapColumns(Vec<u16>),
    /// One sector row of the map: `Some(true)` = good, `Some(false)` = missing.
    SectorMapRow {
        head: u8,
        sector: u32,
        cells: Vec<(u16, bool)>,
    },
    Found {
        good: u32,
        total: u32,
    },
    Pin {
        pin: u32,
        level: String,
    },
    Elapsed(f64),
    Warning(String),
    Plain,
}

impl Output {
    /// Whether the line reports an error (for console colouring).
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Output::CommandFailed(_)
                | Output::FatalHeader
                | Output::VerifyFailed { .. }
                | Output::DeviceNotFound
                | Output::Track { outcome: TrackOutcome::GaveUp { .. }, .. }
        )
    }

    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Output::Warning(_)
                | Output::UpdateSkipped(_)
                | Output::FirmwareUpdate(_)
                | Output::Track { outcome: TrackOutcome::OutOfRange, .. }
                | Output::Track { outcome: TrackOutcome::Decoded { retry: true, .. }, .. }
                | Output::Track { outcome: TrackOutcome::Writing { verify_retry: Some(_) }, .. }
        ) || matches!(self, Output::Track { outcome: TrackOutcome::Decoded { ok, total, .. }, .. } if ok < total)
    }
}

/// Stateful line parser (the sector map spans several lines).
#[derive(Default, Debug)]
pub struct OutputParser {
    tens: Option<String>,
    columns: Vec<u16>,
    after_fatal: bool,
    after_skip: bool,
    in_info_device: bool,
}

impl OutputParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Message of the last `** FATAL ERROR:` if this line is its continuation.
    pub fn is_fatal_message(&self) -> bool {
        self.after_fatal
    }

    pub fn feed(&mut self, raw: &str) -> Output {
        let line = raw.trim_end();
        let fatal_continuation = std::mem::take(&mut self.after_fatal);
        if fatal_continuation && line.contains("Cannot find the Greaseweazle device") {
            return Output::DeviceNotFound;
        }
        if std::mem::take(&mut self.after_skip) {
            return Output::UpdateSkipped(line.trim().to_string());
        }

        if let Some(c) = TRACK_RE.captures(line) {
            let (Ok(cyl), Ok(head)) = (c[1].parse(), c[2].parse()) else {
                return Output::Plain;
            };
            return Output::Track { cyl, head, outcome: track_outcome(&c[3]) };
        }
        if line == "** FATAL ERROR:" {
            self.after_fatal = true;
            return Output::FatalHeader;
        }
        if let Some(msg) = line.strip_prefix("Command Failed: ") {
            return Output::CommandFailed(msg.to_string());
        }
        // Failures that gw reports with exit status 0 (update, bandwidth, mode checks).
        if let Some(msg) = line.strip_prefix("ERROR: ") {
            return Output::CommandFailed(msg.trim().to_string());
        }
        if line.starts_with("** UPDATE FAILED") {
            return Output::CommandFailed(line.trim_start_matches('*').trim().to_string());
        }
        if line == "** SKIPPING UPDATE:" {
            self.after_skip = true;
            return Output::Warning(line.to_string());
        }
        if line.starts_with("** ") {
            return Output::Warning(line.trim_start_matches('*').trim().to_string());
        }
        if line.starts_with("No tracks verified") || line.contains("*not* verified") {
            return Output::Warning(line.to_string());
        }
        if let Some(c) = GEOMETRY_RE.captures(line) {
            if let Ok(ts) = TrackSet::parse(&c[2]) {
                return Output::Geometry(ts);
            }
        }
        if let Some(c) = VERIFY_FAIL_RE.captures(line) {
            if let (Ok(cyl), Ok(head)) = (c[1].parse(), c[2].parse()) {
                return Output::VerifyFailed { cyl, head };
            }
        }
        if line == "All tracks verified" {
            return Output::AllVerified;
        }
        if let Some(c) = RPM_RE.captures(line) {
            let rpm = c[2].parse().unwrap_or(0.0);
            return match c.get(1) {
                Some(label) => Output::RpmStat { label: label.as_str().to_string(), rpm },
                None => Output::Rpm { rpm, ms: c[3].parse().unwrap_or(0.0) },
            };
        }
        if let Some(c) = BANDWIDTH_RE.captures(line) {
            return Output::Bandwidth {
                direction: c[1].to_string(),
                min: c[2].parse().unwrap_or(0.0),
                mean: c[3].parse().unwrap_or(0.0),
                max: c[4].parse().unwrap_or(0.0),
            };
        }
        if let Some(rest) = line.strip_prefix("Cyl-> ") {
            self.tens = Some(rest.to_string());
            self.columns.clear();
            return Output::Plain;
        }
        if let Some(units) = line.strip_prefix("H. S: ") {
            if let Some(tens) = self.tens.take() {
                self.columns = map_columns(&tens, units);
                return Output::SectorMapColumns(self.columns.clone());
            }
        }
        if !self.columns.is_empty() {
            if let Some(c) = MAP_ROW_RE.captures(line) {
                let head = c[1].parse().unwrap_or(0);
                let sector = c[2].parse().unwrap_or(0);
                let cells = c[3]
                    .chars()
                    .zip(self.columns.iter())
                    .filter_map(|(ch, &cyl)| match ch {
                        '.' => Some((cyl, true)),
                        'X' => Some((cyl, false)),
                        _ => None,
                    })
                    .collect();
                return Output::SectorMapRow { head, sector, cells };
            }
        }
        if let Some(c) = FOUND_RE.captures(line) {
            self.columns.clear();
            return Output::Found { good: c[1].parse().unwrap_or(0), total: c[2].parse().unwrap_or(0) };
        }
        if let Some(c) = PIN_RE.captures(line) {
            return Output::Pin { pin: c[1].parse().unwrap_or(0), level: c[2].to_string() };
        }
        if let Some(c) = FIRMWARE_RE.captures(line) {
            return Output::FirmwareUpdate(c[1].to_string());
        }
        if let Some(c) = ELAPSED_RE.captures(line) {
            return Output::Elapsed(c[1].parse().unwrap_or(0.0));
        }
        if line == "Device:" {
            self.in_info_device = true;
            return Output::Plain;
        }
        if self.in_info_device && line.trim() == "Not found" {
            self.in_info_device = false;
            return Output::DeviceNotFound;
        }
        if line.contains("**WARNING**") || line.starts_with("WARNING") {
            return Output::Warning(line.trim().to_string());
        }
        if let Some(c) = KV_RE.captures(line) {
            return Output::KeyValue { key: c[1].trim().to_string(), value: c[2].trim().to_string() };
        }
        Output::Plain
    }
}

fn track_outcome(body: &str) -> TrackOutcome {
    if let Some(c) = MISSING_RE.captures(body) {
        return TrackOutcome::GaveUp { missing: c[1].parse().unwrap_or(0) };
    }
    if body.starts_with("WARNING: Out of range") {
        return TrackOutcome::OutOfRange;
    }
    if body.starts_with("Writing Track") {
        let verify_retry = VERIFY_RETRY_RE.captures(body).and_then(|c| c[1].parse().ok());
        return TrackOutcome::Writing { verify_retry };
    }
    if body.starts_with("Erasing Track") {
        return TrackOutcome::Erasing;
    }
    if body.starts_with("IBM Empty") {
        return TrackOutcome::Empty;
    }
    if let Some(c) = SECTORS_RE.captures(body) {
        let ok = c[1].parse().unwrap_or(0);
        let total = c[2].parse().unwrap_or(0);
        if total == 0 {
            return TrackOutcome::Empty;
        }
        return TrackOutcome::Decoded { ok, total, retry: body.contains("(Retry #") };
    }
    if body.starts_with("Raw Flux") || body.contains("hard sectors") {
        return TrackOutcome::Flux;
    }
    TrackOutcome::Other
}

/// Rebuild cylinder numbers from the two header rows of gw's sector map
/// (`Cyl-> 0         1…` and `H. S: 0123456789…`).
fn map_columns(tens: &str, units: &str) -> Vec<u16> {
    let mut out = Vec::new();
    let mut current = 0u16;
    let mut tens_chars = tens.chars();
    for u in units.chars() {
        if let Some(t) = tens_chars.next().and_then(|c| c.to_digit(10)) {
            current = t as u16;
        }
        match u.to_digit(10) {
            Some(d) => out.push(current * 10 + d as u16),
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Output {
        OutputParser::new().feed(line)
    }

    #[test]
    fn read_lines() {
        assert_eq!(
            one("T12.1: AmigaDOS (11/11 sectors) from Raw Flux (101082 flux in 400.00ms)"),
            Output::Track {
                cyl: 12,
                head: 1,
                outcome: TrackOutcome::Decoded { ok: 11, total: 11, retry: false }
            }
        );
        assert_eq!(
            one("T0.0: IBM MFM (7/18 sectors) from Raw Flux (1 flux in 2ms) (Retry #1.3)"),
            Output::Track {
                cyl: 0,
                head: 0,
                outcome: TrackOutcome::Decoded { ok: 7, total: 18, retry: true }
            }
        );
        assert_eq!(
            one("T3.0 <- Drive 6.0: Giving up: 3 sectors missing"),
            Output::Track { cyl: 3, head: 0, outcome: TrackOutcome::GaveUp { missing: 3 } }
        );
        assert_eq!(
            one("T5.1: Raw Flux (97556 flux in 600.00ms)"),
            Output::Track { cyl: 5, head: 1, outcome: TrackOutcome::Flux }
        );
        assert_eq!(
            one("T2.0: IBM Empty from Raw Flux (1 flux in 2ms)"),
            Output::Track { cyl: 2, head: 0, outcome: TrackOutcome::Empty }
        );
    }

    #[test]
    fn write_and_erase_lines() {
        assert_eq!(
            one("T7.0: Writing Track (Verify Failure: Retry #2)"),
            Output::Track { cyl: 7, head: 0, outcome: TrackOutcome::Writing { verify_retry: Some(2) } }
        );
        assert_eq!(
            one("T7.1: Erasing Track"),
            Output::Track { cyl: 7, head: 1, outcome: TrackOutcome::Erasing }
        );
        assert_eq!(one("Failed to verify Track 9.1"), Output::VerifyFailed { cyl: 9, head: 1 });
        assert!(matches!(one("Erasing c=0-79:h=0-1, revs=1"), Output::Geometry(t) if t.cyls.len() == 80));
        assert!(matches!(one("Writing c=0-39:h=0:step=2"), Output::Geometry(t) if t.heads == vec![0]));
    }

    #[test]
    fn device_lines() {
        let mut p = OutputParser::new();
        assert_eq!(
            p.feed("Host Tools: 1.23"),
            Output::KeyValue { key: "Host Tools".into(), value: "1.23".into() }
        );
        p.feed("Device:");
        assert_eq!(p.feed("  Not found"), Output::DeviceNotFound);
        let mut p = OutputParser::new();
        p.feed("Device:");
        assert_eq!(
            p.feed("  Model:    Greaseweazle V4.1"),
            Output::KeyValue { key: "Model".into(), value: "Greaseweazle V4.1".into() }
        );
        assert_eq!(p.feed("*** New firmware version 1.6 is available"), Output::FirmwareUpdate("1.6".into()));
        let mut p = OutputParser::new();
        assert_eq!(p.feed("** FATAL ERROR:"), Output::FatalHeader);
        assert_eq!(p.feed("Cannot find the Greaseweazle device"), Output::DeviceNotFound);
        assert!(matches!(one("Command Failed: No Index"), Output::CommandFailed(m) if m == "No Index"));
    }

    #[test]
    fn failures_with_exit_status_zero() {
        assert!(
            matches!(one("ERROR: Device is in Firmware Update Mode"), Output::CommandFailed(m) if m.contains("Update Mode"))
        );
        assert!(
            matches!(one("** UPDATE FAILED: Please retry!"), Output::CommandFailed(m) if m.starts_with("UPDATE FAILED"))
        );
        assert!(one("ERROR: USB read data garbled (Device -> Host)").is_error());
        let mut p = OutputParser::new();
        assert!(p.feed("** SKIPPING UPDATE:").is_warning());
        assert!(
            matches!(p.feed("Device is already running version 1.6."), Output::UpdateSkipped(m) if m.contains("1.6"))
        );
        assert!(one("** Unplug device and remove the Update Jumper").is_warning());
        assert!(one("3 tracks verified; 2 tracks *not* verified (Reason: Verify unavailable)").is_warning());
    }

    #[test]
    fn rpm_bandwidth_pin() {
        assert_eq!(one("Rate: 300.123 rpm ; Period: 199.918 ms"), Output::Rpm { rpm: 300.123, ms: 199.918 });
        assert_eq!(
            one("Median:   Rate: 299.900 rpm ; Period: 200.067 ms"),
            Output::RpmStat { label: "Median".into(), rpm: 299.9 }
        );
        assert_eq!(
            one("Read Bandwidth:     9.123 /    9.500 /   10.000 Mbps"),
            Output::Bandwidth { direction: "Read".into(), min: 9.123, mean: 9.5, max: 10.0 }
        );
        assert_eq!(one("Pin 2 is High"), Output::Pin { pin: 2, level: "High".into() });
        assert_eq!(
            one("Select Delay: 10us"),
            Output::KeyValue { key: "Select Delay".into(), value: "10us".into() }
        );
    }

    #[test]
    fn sector_map() {
        let mut p = OutputParser::new();
        p.feed("Cyl-> 0         1");
        assert_eq!(p.feed("H. S: 012345678901"), Output::SectorMapColumns((0..12).collect()));
        match p.feed("1. 3: ..X.       .") {
            Output::SectorMapRow { head, sector, cells } => {
                assert_eq!((head, sector), (1, 3));
                assert_eq!(cells[2], (2, false));
                assert_eq!(cells.last(), Some(&(11, true)));
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(p.feed("Found 1750 sectors of 1760 (99%)"), Output::Found { good: 1750, total: 1760 });
    }
}
