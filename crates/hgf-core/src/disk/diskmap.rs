// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-track state of the disk being read, written, converted or erased.

use crate::disk::trackset::TrackSet;
use crate::gw::parse::{Output, TrackOutcome};
use std::collections::BTreeMap;

/// Upper bound on the sector numbers accepted from gw's sector map.
const MAX_SECTORS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackState {
    /// Not processed yet.
    #[default]
    Pending,
    /// Being processed right now.
    Active,
    /// All sectors decoded / written and verified / erased.
    Ok,
    /// All sectors decoded, but only after retries (marginal track).
    Recovered,
    /// Some sectors still missing, gw is retrying.
    Retrying,
    /// gw gave up or verification failed.
    Bad,
    /// No data on the track (unformatted or `ibm.scan` found nothing).
    Empty,
    /// Raw flux captured without decoding.
    Flux,
    /// Track outside the format definition.
    OutOfRange,
}

impl TrackState {
    pub fn label(self) -> &'static str {
        match self {
            TrackState::Pending => "Pending",
            TrackState::Active => "In progress",
            TrackState::Ok => "OK",
            TrackState::Recovered => "Recovered after retries",
            TrackState::Retrying => "Retrying",
            TrackState::Bad => "Bad",
            TrackState::Empty => "Empty",
            TrackState::Flux => "Flux captured",
            TrackState::OutOfRange => "Out of format range",
        }
    }

    /// Whether gw is done with this track.
    pub fn is_final(self) -> bool {
        !matches!(self, TrackState::Pending | TrackState::Active | TrackState::Retrying)
    }
}

/// What kind of operation drives the map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapMode {
    #[default]
    Read,
    Write,
    Erase,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackInfo {
    pub state: TrackState,
    /// Decoded sectors (good, total), when known.
    pub sectors: Option<(u32, u32)>,
    /// Number of retry passes seen for this track.
    pub retries: u32,
    /// Per-sector result from gw's final sector map (`true` = good).
    pub sector_map: Vec<Option<bool>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub ok: usize,
    pub recovered: usize,
    pub bad: usize,
    pub empty: usize,
    pub flux: usize,
    pub pending: usize,
}

#[derive(Clone, Debug)]
pub struct DiskMap {
    pub mode: MapMode,
    pub geometry: TrackSet,
    /// `true` once gw reported the real track set.
    pub geometry_known: bool,
    tracks: BTreeMap<(u16, u8), TrackInfo>,
    pub active: Option<(u16, u8)>,
    /// `Found X sectors of Y` summary.
    pub found: Option<(u32, u32)>,
    /// Tracks being re-read: the full-disk geometry and results are kept.
    pub reread: Option<Vec<(u16, u8)>>,
}

impl Default for DiskMap {
    fn default() -> Self {
        Self::new(MapMode::Read, TrackSet::dense(80, 2))
    }
}

impl DiskMap {
    /// A blank map with a provisional geometry (replaced when gw reports the real one).
    pub fn new(mode: MapMode, provisional: TrackSet) -> Self {
        Self {
            mode,
            geometry: provisional,
            geometry_known: false,
            tracks: BTreeMap::new(),
            active: None,
            found: None,
            reread: None,
        }
    }

    pub fn get(&self, cyl: u16, head: u8) -> TrackInfo {
        self.tracks.get(&(cyl, head)).cloned().unwrap_or_default()
    }

    fn entry(&mut self, cyl: u16, head: u8) -> &mut TrackInfo {
        if !self.geometry.cyls.contains(&cyl) {
            let pos = self.geometry.cyls.partition_point(|&c| c < cyl);
            self.geometry.cyls.insert(pos, cyl);
        }
        if !self.geometry.heads.contains(&head) {
            let pos = self.geometry.heads.partition_point(|&h| h < head);
            self.geometry.heads.insert(pos, head);
        }
        self.tracks.entry((cyl, head)).or_default()
    }

    /// Update the map from one parsed output line.
    pub fn apply(&mut self, out: &Output) {
        match out {
            Output::Geometry(ts) => {
                if self.reread.is_none() {
                    self.geometry = ts.clone();
                    self.geometry_known = true;
                }
            }
            Output::Track { cyl, head, outcome } => self.apply_track(*cyl, *head, outcome),
            Output::VerifyFailed { cyl, head } => {
                self.entry(*cyl, *head).state = TrackState::Bad;
                self.active = None;
            }
            Output::SectorMapRow { head, sector, cells } => {
                for &(cyl, good) in cells {
                    let t = self.entry(cyl, *head);
                    let idx = *sector as usize;
                    // No floppy format has more than a few dozen sectors per track.
                    if idx >= MAX_SECTORS {
                        continue;
                    }
                    if t.sector_map.len() <= idx {
                        t.sector_map.resize(idx + 1, None);
                    }
                    t.sector_map[idx] = Some(good);
                }
            }
            Output::Found { good, total } if self.reread.is_none() => self.found = Some((*good, *total)),
            _ => {}
        }
    }

    fn apply_track(&mut self, cyl: u16, head: u8, outcome: &TrackOutcome) {
        // Write/erase report the start of a track: the previous one completed successfully.
        // A read track left with missing sectors (gw convert never retries) is bad.
        if let Some(prev) = self.active.filter(|&p| p != (cyl, head)) {
            let mode = self.mode;
            let t = self.entry(prev.0, prev.1);
            if t.state == TrackState::Retrying {
                t.state = TrackState::Bad;
            } else if mode != MapMode::Read && !t.state.is_final() {
                t.state = if t.retries > 0 { TrackState::Recovered } else { TrackState::Ok };
            }
        }
        self.active = Some((cyl, head));
        let mode = self.mode;
        let t = self.entry(cyl, head);
        match outcome {
            TrackOutcome::Decoded { ok, total, retry } => {
                t.sectors = Some((*ok, *total));
                if *retry {
                    t.retries += 1;
                }
                t.state = if ok >= total {
                    if t.retries > 0 {
                        TrackState::Recovered
                    } else {
                        TrackState::Ok
                    }
                } else {
                    TrackState::Retrying
                };
            }
            TrackOutcome::GaveUp { .. } => t.state = TrackState::Bad,
            TrackOutcome::Flux => t.state = TrackState::Flux,
            TrackOutcome::OutOfRange => t.state = TrackState::OutOfRange,
            TrackOutcome::Empty => t.state = TrackState::Empty,
            TrackOutcome::Writing { verify_retry } => {
                if verify_retry.is_some() {
                    t.retries += 1;
                }
                t.state = TrackState::Active;
            }
            TrackOutcome::Erasing => {
                if mode == MapMode::Erase || t.state == TrackState::Pending {
                    t.state = TrackState::Active;
                }
            }
            TrackOutcome::Other => {
                if t.state == TrackState::Pending {
                    t.state = TrackState::Active;
                }
            }
        }
    }

    /// Close the map when gw exits.
    pub fn finish(&mut self, success: bool) {
        if let Some((c, h)) = self.active.take() {
            let t = self.entry(c, h);
            if t.state == TrackState::Retrying {
                t.state = TrackState::Bad;
            } else if !t.state.is_final() {
                t.state = match (success, t.retries) {
                    (true, 0) => TrackState::Ok,
                    (true, _) => TrackState::Recovered,
                    (false, _) => TrackState::Bad,
                };
            }
        }
    }

    /// Tracks gw gave up on (or that failed verification).
    pub fn bad_tracks(&self) -> Vec<(u16, u8)> {
        let mut v = Vec::new();
        for &c in &self.geometry.cyls {
            for &h in &self.geometry.heads {
                if self.get(c, h).state == TrackState::Bad {
                    v.push((c, h));
                }
            }
        }
        v
    }

    /// Prepare a re-read of the bad tracks. Returns the TSPEC to pass to gw (`c=…:h=…`).
    ///
    /// gw track sets are products of cylinders and heads, so a few good tracks may be read
    /// again too; their state is simply refreshed.
    pub fn start_reread(&mut self) -> Option<String> {
        let bad = self.bad_tracks();
        if bad.is_empty() {
            return None;
        }
        let mut cyls: Vec<u16> = bad.iter().map(|t| t.0).collect();
        let mut heads: Vec<u8> = bad.iter().map(|t| t.1).collect();
        cyls.sort_unstable();
        cyls.dedup();
        heads.sort_unstable();
        heads.dedup();
        for &(c, h) in &bad {
            self.tracks.insert((c, h), TrackInfo::default());
        }
        self.mode = MapMode::Read;
        self.active = None;
        self.found = None;
        self.reread = Some(cyls.iter().flat_map(|&c| heads.iter().map(move |&h| (c, h))).collect());
        Some(TrackSet { cyls, heads }.to_string())
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for &cyl in &self.geometry.cyls {
            for &head in &self.geometry.heads {
                match self.get(cyl, head).state {
                    TrackState::Ok => c.ok += 1,
                    TrackState::Recovered => c.recovered += 1,
                    TrackState::Bad => c.bad += 1,
                    TrackState::Empty | TrackState::OutOfRange => c.empty += 1,
                    TrackState::Flux => c.flux += 1,
                    TrackState::Pending | TrackState::Active | TrackState::Retrying => c.pending += 1,
                }
            }
        }
        c
    }

    /// Fraction of tracks gw is done with.
    pub fn progress(&self) -> f32 {
        if let Some(set) = &self.reread {
            let done = set.iter().filter(|&&(c, h)| self.get(c, h).state.is_final()).count();
            return done as f32 / set.len().max(1) as f32;
        }
        let total = self.geometry.len();
        if total == 0 {
            return 0.0;
        }
        let done = total - self.counts().pending;
        done as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gw::parse::OutputParser;

    fn feed(map: &mut DiskMap, lines: &[&str]) {
        let mut p = OutputParser::new();
        for l in lines {
            map.apply(&p.feed(l));
        }
    }

    #[test]
    fn read_with_retries() {
        let mut m = DiskMap::default();
        feed(
            &mut m,
            &[
                "Reading c=0-1:h=0 revs=3",
                "T0.0: AmigaDOS (11/11 sectors) from Raw Flux (1 flux in 2ms)",
                "T1.0: AmigaDOS (8/11 sectors) from Raw Flux (1 flux in 2ms)",
            ],
        );
        assert!(m.geometry_known);
        assert_eq!(m.geometry.len(), 2);
        assert_eq!(m.get(0, 0).state, TrackState::Ok);
        assert_eq!(m.get(1, 0).state, TrackState::Retrying);
        assert!(m.progress() > 0.49 && m.progress() < 0.51);
        feed(&mut m, &["T1.0: AmigaDOS (11/11 sectors) from Raw Flux (1 flux in 2ms) (Retry #0.1)"]);
        assert_eq!(m.get(1, 0).state, TrackState::Recovered);
        assert_eq!(m.get(1, 0).retries, 1);
        assert_eq!(m.progress(), 1.0);
    }

    #[test]
    fn read_giving_up() {
        let mut m = DiskMap::default();
        feed(
            &mut m,
            &[
                "T4.1: IBM MFM (3/18 sectors) from Raw Flux (1 flux in 2ms)",
                "T4.1: Giving up: 15 sectors missing",
            ],
        );
        assert_eq!(m.get(4, 1).state, TrackState::Bad);
    }

    #[test]
    fn reread_keeps_the_rest_of_the_disk() {
        let mut m = DiskMap::default();
        feed(
            &mut m,
            &[
                "Reading c=0-79:h=0-1 revs=1.1",
                "T0.0: AmigaDOS (11/11 sectors) from Raw Flux (1 flux in 2ms)",
                "T53.1: AmigaDOS (10/11 sectors) from Raw Flux (1 flux in 2ms)",
                "T53.1: Giving up: 1 sectors missing",
                "T57.1: AmigaDOS (10/11 sectors) from Raw Flux (1 flux in 2ms)",
                "T57.1: Giving up: 1 sectors missing",
            ],
        );
        m.finish(true);
        assert_eq!(m.bad_tracks(), vec![(53, 1), (57, 1)]);
        assert_eq!(m.start_reread().as_deref(), Some("c=53,57:h=1"));
        assert_eq!(m.progress(), 0.0);
        feed(
            &mut m,
            &[
                "Reading c=53,57:h=1 revs=5",
                "T53.1: AmigaDOS (11/11 sectors) from Raw Flux (1 flux in 2ms) (Retry #0.2)",
                "T57.1: AmigaDOS (10/11 sectors) from Raw Flux (1 flux in 2ms)",
                "T57.1: Giving up: 1 sectors missing",
                "Found 21 sectors of 22 (95%)",
            ],
        );
        m.finish(true);
        assert_eq!(m.geometry.cyls.len(), 80, "full geometry kept");
        assert_eq!(m.get(0, 0).state, TrackState::Ok, "untouched track kept");
        assert_eq!(m.get(53, 1).state, TrackState::Recovered);
        assert_eq!(m.get(57, 1).state, TrackState::Bad);
        assert_eq!(m.found, None, "partial totals are not shown as the disk total");
        assert_eq!(m.progress(), 1.0);
    }

    #[test]
    fn write_marks_previous_track_ok() {
        let mut m = DiskMap::new(MapMode::Write, TrackSet::dense(2, 1));
        feed(
            &mut m,
            &[
                "T0.0: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)",
                "T0.0: Writing Track (Verify Failure: Retry #1)",
                "T1.0: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)",
            ],
        );
        assert_eq!(m.get(0, 0).state, TrackState::Recovered);
        assert_eq!(m.get(1, 0).state, TrackState::Active);
        m.finish(true);
        assert_eq!(m.get(1, 0).state, TrackState::Ok);
    }

    #[test]
    fn write_verify_failure() {
        let mut m = DiskMap::new(MapMode::Write, TrackSet::dense(2, 1));
        feed(&mut m, &["T1.0: Writing Track (x)", "** FATAL ERROR:", "Failed to verify Track 1.0"]);
        m.finish(false);
        assert_eq!(m.get(1, 0).state, TrackState::Bad);
    }
}
