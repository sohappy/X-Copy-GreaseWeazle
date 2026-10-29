// SPDX-License-Identifier: GPL-3.0-or-later

//! Drive health check: interpretation of each test and the final verdict.
//!
//! The GUI runs the tests (gw info, rpm, bandwidth, reads of a reference disk, head-offset
//! reads, write + read-back of a scratch disk); this module only turns their results into
//! findings, so the logic is unit-tested with real captures.

use crate::disk::diskmap::DiskMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub status: Status,
    pub detail: String,
}

impl Finding {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self { status, detail: detail.into() }
    }
}

/// Decoded sectors of a read, overall and per side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub good: u32,
    pub total: u32,
    pub per_head: [(u32, u32); 2],
    /// Good sectors on the lowest cylinder read (the drive re-calibrates there).
    pub first_cyl_good: u32,
}

impl ScanStats {
    pub fn from_map(map: &DiskMap) -> Self {
        let mut s = ScanStats::default();
        let first = map.geometry.cyls.first().copied();
        for &c in &map.geometry.cyls {
            for &h in &map.geometry.heads {
                if let Some((ok, total)) = map.get(c, h).sectors {
                    s.good += ok;
                    s.total += total;
                    let slot = &mut s.per_head[(h as usize).min(1)];
                    slot.0 += ok;
                    slot.1 += total;
                    if Some(c) == first {
                        s.first_cyl_good += ok;
                    }
                }
            }
        }
        s
    }

    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.good as f64 / self.total as f64
        }
    }

    pub fn head_ratio(&self, h: usize) -> Option<f64> {
        let (g, t) = self.per_head[h];
        (t > 0).then(|| g as f64 / t as f64)
    }
}

/// What the reference read suggests, used to decide the next tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScanPattern {
    Good,
    /// Mostly readable; `weak_side` is the side with noticeably more errors.
    Marginal {
        weak_side: Option<u8>,
    },
    /// One side reads, the other does not.
    SideFailing(u8),
    /// Almost nothing decodes: blank disk, wrong format or head positioning.
    NothingDecoded,
    Poor,
}

/// Motor speed from `gw rpm` samples.
pub fn rpm(samples: &[f64]) -> Finding {
    if samples.is_empty() {
        return Finding::new(
            Status::Fail,
            "No index pulse. Insert a disk until it clicks, check the drive power and the \
             drive selection (A after the cable twist, B on the middle connector).",
        );
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let nominal = if (mean - 360.0).abs() < (mean - 300.0).abs() { 360.0 } else { 300.0 };
    let dev = (mean - nominal) / nominal * 100.0;
    let (lo, hi) = samples.iter().fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let wobble = (hi - lo) / nominal * 100.0;
    let text = format!("{mean:.2} rpm ({dev:+.2}% vs {nominal:.0}), variation {wobble:.2}%.");
    if dev.abs() < 1.5 && wobble < 1.0 {
        Finding::new(Status::Pass, text)
    } else if dev.abs() < 3.0 && wobble < 2.0 {
        Finding::new(Status::Warn, format!("{text} Slightly out of tolerance; reads may be marginal."))
    } else {
        Finding::new(Status::Fail, format!("{text} Out of tolerance: belt, motor or drive fault."))
    }
}

/// USB link from `gw bandwidth` (min/mean/max rows and notes).
pub fn bandwidth(rows: &[(String, f64, f64, f64)], notes: &[String]) -> Finding {
    let problem =
        notes.iter().find(|n| n.contains("WARNING") || n.contains("garbled") || n.contains("ERROR"));
    let min = rows.iter().map(|r| r.1).fold(f64::MAX, f64::min);
    if rows.is_empty() {
        return Finding::new(Status::Fail, "No bandwidth figures: the USB link could not be measured.");
    }
    match problem {
        Some(p) => Finding::new(
            Status::Warn,
            format!("Minimum {min:.2} Mbps. {p} Use a direct USB port and a short cable."),
        ),
        None => {
            Finding::new(Status::Pass, format!("Minimum {min:.2} Mbps, stable: enough for DD and HD disks."))
        }
    }
}

/// Reference disk read (one pass, no retries).
pub fn reference(s: &ScanStats) -> (Finding, ScanPattern) {
    if s.total == 0 {
        return (
            Finding::new(Status::Fail, "Nothing decoded: check the disk format selected for the test."),
            ScanPattern::NothingDecoded,
        );
    }
    let r = s.ratio();
    let pct = r * 100.0;
    let heads: Vec<(u8, f64)> = (0..2).filter_map(|h| s.head_ratio(h).map(|v| (h as u8, v))).collect();
    let summary = format!("{} of {} sectors read in one pass ({pct:.1}%).", s.good, s.total);
    if heads.len() == 2 {
        let (a, b) = (heads[0].1, heads[1].1);
        if a < 0.5 && b > 0.9 || b < 0.5 && a > 0.9 {
            let bad = if a < b { 0 } else { 1 };
            return (
                Finding::new(
                    Status::Fail,
                    format!("{summary} Side {bad} is unreadable while the other side reads."),
                ),
                ScanPattern::SideFailing(bad),
            );
        }
    }
    if r < 0.10 {
        return (
            Finding::new(Status::Fail, format!("{summary} Almost nothing decodes.")),
            ScanPattern::NothingDecoded,
        );
    }
    let weak_side = if heads.len() == 2 && (heads[0].1 - heads[1].1).abs() > 0.002 {
        Some(if heads[0].1 < heads[1].1 { heads[0].0 } else { heads[1].0 })
    } else {
        None
    };
    if r >= 0.98 {
        let extra = match weak_side {
            Some(h) if r < 1.0 => format!(" The few missing sectors are mostly on side {h}."),
            _ => String::new(),
        };
        (Finding::new(Status::Pass, format!("{summary}{extra}")), ScanPattern::Good)
    } else if r >= 0.85 {
        (
            Finding::new(
                Status::Warn,
                format!("{summary} Weak sectors (no retries were made during this test)."),
            ),
            ScanPattern::Marginal { weak_side },
        )
    } else {
        (Finding::new(Status::Fail, format!("{summary} Many sectors unreadable.")), ScanPattern::Poor)
    }
}

/// Reads of cylinders 2-10 with the head moved one track out (+1) and in (-1).
pub fn offset(plus: &ScanStats, minus: &ScanStats) -> (Finding, Option<i8>) {
    let (rp, rm) = (plus.ratio(), minus.ratio());
    let best = if rp >= rm { (rp, 1) } else { (rm, -1) };
    if best.0 > 0.8 {
        (
            Finding::new(
                Status::Fail,
                format!(
                    "Data found one track {} ({:.0}% decoded): the head is offset by one track \
                     (track-0 sensor misadjusted or missed steps).",
                    if best.1 > 0 { "further in" } else { "further out" },
                    best.0 * 100.0
                ),
            ),
            Some(best.1),
        )
    } else {
        (
            Finding::new(
                Status::Pass,
                "No data one track further in or out: the head is not offset. The reference disk is \
                 probably blank, erased, or not in the selected format.",
            ),
            None,
        )
    }
}

/// Write of a blank image to a scratch disk, then a read-back.
pub fn write_test(write_ok: bool, failure: Option<&str>, readback: Option<&ScanStats>) -> Finding {
    if !write_ok {
        return Finding::new(
            Status::Fail,
            format!(
                "Write failed{}. The drive reads but does not write reliably, or the scratch disk is bad.",
                failure.map(|f| format!(" ({f})")).unwrap_or_default()
            ),
        );
    }
    match readback {
        Some(s) if s.ratio() >= 0.98 => Finding::new(
            Status::Pass,
            format!("Written, verified and read back: {:.1}% of sectors.", s.ratio() * 100.0),
        ),
        Some(s) => Finding::new(
            Status::Warn,
            format!("Written and verified, but only {:.1}% read back in one pass.", s.ratio() * 100.0),
        ),
        None => Finding::new(Status::Pass, "Written and verified."),
    }
}

/// Everything the wizard learned.
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub device: Option<bool>,
    pub rpm: Option<Finding>,
    pub bandwidth: Option<Finding>,
    pub reference: Option<(Finding, ScanPattern)>,
    pub offset: Option<(Finding, Option<i8>)>,
    pub write: Option<Finding>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub status: Status,
    pub headline: String,
    pub advice: Vec<String>,
}

pub fn verdict(r: &Results) -> Verdict {
    let v = |status, headline: &str, advice: &[&str]| Verdict {
        status,
        headline: headline.to_string(),
        advice: advice.iter().map(|s| s.to_string()).collect(),
    };
    if r.device == Some(false) {
        return v(
            Status::Fail,
            "No Greaseweazle detected",
            &["Check the USB cable and try another port.", "Run Device info to see what gw reports."],
        );
    }
    if matches!(&r.rpm, Some(f) if f.status == Status::Fail && f.detail.starts_with("No index")) {
        return v(
            Status::Fail,
            "The drive sees no disk (no index pulse)",
            &[
                "Insert the disk until it clicks, shutter side first.",
                "Check the 4-pin power connector of the drive.",
                "With a twisted cable: Drive A after the twist, Drive B on the middle connector.",
                "A drive LED that stays on permanently means the cable is plugged upside down.",
            ],
        );
    }
    if let Some((_, Some(dir))) = &r.offset {
        return v(Status::Fail, "Head positioning fault", &[
            &format!("Data sits one track {} of where the drive expects it.", if *dir > 0 { "further in" } else { "further out" }),
            "Try longer step and settle delays (Drive delays: step 15000 µs, settle 40 ms) and run the check again.",
            "Otherwise the track-0 sensor needs adjusting, or use another drive.",
            "Do not write with this drive until it is fixed.",
        ]);
    }
    if let Some((_, ScanPattern::SideFailing(h))) = &r.reference {
        return Verdict {
            status: Status::Fail,
            headline: format!("Side {h} does not read: head {h} suspected"),
            advice: vec![
                "Clean the heads with a cleaning disk (Clean drive), then run the check again.".into(),
                format!("Try another reference disk: if side {h} still fails, the head is worn or damaged."),
            ],
        };
    }
    if let Some((_, ScanPattern::NothingDecoded)) = &r.reference {
        return v(Status::Warn, "The reference disk could not be read in this format", &[
            "The head is not offset, so the drive is probably fine.",
            "Use a disk you know is good in the selected format (an original that boots on its machine, or one fully formatted — not Quick Format).",
            "Check that the selected format matches the disk.",
        ]);
    }
    if matches!(&r.write, Some(f) if f.status == Status::Fail) {
        return v(
            Status::Fail,
            "The drive reads but writing fails",
            &[
                "Try another scratch disk (DD media for DD formats, HD hole taped if needed).",
                "Clean the heads, then run the check again.",
            ],
        );
    }
    if matches!(&r.rpm, Some(f) if f.status != Status::Pass) {
        return v(
            Status::Warn,
            "Motor speed out of tolerance",
            &["Speed problems cause marginal reads and writes; a worn belt or old grease is typical."],
        );
    }
    if matches!(&r.reference, Some((_, ScanPattern::Poor))) {
        return v(
            Status::Warn,
            "Many read errors on the reference disk",
            &["Clean the heads and try another reference disk to tell the drive from the disk apart."],
        );
    }
    let mut advice = Vec::new();
    let mut status = Status::Pass;
    let mut headline = "Drive healthy".to_string();
    if let Some((_, ScanPattern::Marginal { weak_side })) = &r.reference {
        status = Status::Warn;
        headline = match weak_side {
            Some(h) => format!("Drive OK — reference disk worn on side {h}"),
            None => "Drive OK — reference disk has weak sectors".into(),
        };
        advice.push("Read it with the Recovery profile to see which sectors are really lost.".into());
    }
    if matches!(&r.bandwidth, Some(f) if f.status != Status::Pass) {
        status = Status::Warn;
        advice.push("USB bandwidth is marginal: use a direct port and a short cable.".into());
    }
    if r.write.is_none() {
        advice.push("Writing was not tested.".into());
    }
    Verdict { status, headline, advice }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::diskmap::MapMode;
    use crate::disk::trackset::TrackSet;
    use crate::gw::parse::OutputParser;

    fn replay(log: &str) -> ScanStats {
        let mut map = DiskMap::new(MapMode::Read, TrackSet::dense(80, 2));
        let mut p = OutputParser::new();
        for l in log.lines() {
            map.apply(&p.feed(l));
        }
        map.finish(true);
        ScanStats::from_map(&map)
    }

    #[test]
    fn rpm_findings() {
        assert_eq!(rpm(&[300.52, 300.54, 300.57]).status, Status::Pass);
        assert_eq!(rpm(&[301.25, 301.31]).status, Status::Pass);
        assert_eq!(rpm(&[306.0, 306.5]).status, Status::Warn);
        assert_eq!(rpm(&[285.0]).status, Status::Fail);
        assert!(rpm(&[]).detail.starts_with("No index"));
        assert_eq!(rpm(&[360.1, 360.2]).status, Status::Pass);
    }

    #[test]
    fn real_worn_disk_is_marginal_side_1() {
        // Capture from a real drive: 1745/1760 sectors, errors clustered on side 1.
        let s = replay(include_str!("../../tests/fixtures/real_scan_worn.log"));
        assert_eq!((s.good, s.total), (1745, 1760));
        let (f, pattern) = reference(&s);
        assert_eq!(f.status, Status::Pass);
        assert_eq!(pattern, ScanPattern::Good);
        assert!(f.detail.contains("side 1"));
    }

    #[test]
    fn real_erased_disk_decodes_nothing() {
        // Capture from a real drive: disk erased by gw, only track 0 partly written.
        let s = replay(include_str!("../../tests/fixtures/real_scan_erased.log"));
        assert_eq!(s.good, 18);
        let (_, pattern) = reference(&s);
        assert_eq!(pattern, ScanPattern::NothingDecoded);
        assert!(s.first_cyl_good > 0);
        let blank = ScanStats { good: 0, total: 198, ..Default::default() };
        let (f, dir) = offset(&blank, &blank);
        assert_eq!((f.status, dir), (Status::Pass, None));
        let r = Results {
            device: Some(true),
            rpm: Some(rpm(&[301.28])),
            reference: Some(reference(&s)),
            offset: Some((f, dir)),
            ..Default::default()
        };
        assert_eq!(verdict(&r).headline, "The reference disk could not be read in this format");
    }

    #[test]
    fn verdicts() {
        let good =
            ScanStats { good: 1760, total: 1760, per_head: [(880, 880), (880, 880)], first_cyl_good: 22 };
        let r = Results {
            device: Some(true),
            rpm: Some(rpm(&[300.5])),
            bandwidth: Some(bandwidth(&[("Write".into(), 8.9, 9.0, 9.1)], &[])),
            reference: Some(reference(&good)),
            offset: None,
            write: Some(write_test(true, None, Some(&good))),
        };
        assert_eq!(verdict(&r).headline, "Drive healthy");

        let side =
            ScanStats { good: 900, total: 1760, per_head: [(880, 880), (20, 880)], first_cyl_good: 12 };
        let r2 = Results { reference: Some(reference(&side)), ..r.clone() };
        assert!(verdict(&r2).headline.contains("Side 1"));

        let shifted = ScanStats { good: 190, total: 198, ..Default::default() };
        let blank = ScanStats { good: 0, total: 198, ..Default::default() };
        let r3 = Results { offset: Some(offset(&blank, &shifted)), ..r.clone() };
        assert_eq!(verdict(&r3).headline, "Head positioning fault");

        let r4 = Results { rpm: Some(rpm(&[])), ..r.clone() };
        assert!(verdict(&r4).headline.contains("no index"));

        let r5 = Results { write: Some(write_test(false, Some("Failed to verify Track 0.1"), None)), ..r };
        assert_eq!(verdict(&r5).headline, "The drive reads but writing fails");
    }
}
