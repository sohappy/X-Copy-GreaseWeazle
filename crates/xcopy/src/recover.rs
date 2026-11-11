// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Automatic re-read of the bad tracks after an AmigaDOS read, with a sector-by-sector merge.
//!
//! After the main read, only the tracks with missing sectors are read again with the recovery
//! profile (more revolutions, retries and head re-seeks), up to [`PASSES`] times. After each
//! pass every sector decoded correctly is copied into the main image: a track is complete as
//! soon as each of its sectors has been read once, even if no single pass read it all.

use crate::engine::{Engine, Level, Task};
use hgf_core::fs::amigados::{sectors_per_track, BSIZE};
use hgf_core::gw::{Action, ConvertOpts, Drive, ReadOpts, ReadProfile, TrackOpts};
use hgf_core::{DiskMap, TrackInfo, TrackSet, TrackState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Folder, beside the image, where the flux of each pass is kept.
pub const PASSES_FOLDER: &str = "Read passes";

/// Where to keep the raw flux of each read pass (`Read passes/<stem> pass N.scp`).
#[derive(Clone, Debug)]
pub struct Keep {
    pub dir: PathBuf,
    pub stem: String,
}

impl Keep {
    /// `dir/Read passes`, created now.
    pub fn new(beside: &Path, stem: &str) -> Result<Keep, String> {
        let dir = beside.join(PASSES_FOLDER);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Keep { dir, stem: stem.to_string() })
    }

    pub fn pass(&self, n: u32) -> PathBuf {
        self.dir.join(format!("{} pass {n}.scp", self.stem))
    }
}

/// Read the raw flux into `flux` (gw still decodes it to retry the tracks with missing
/// sectors), then decode that flux into the sector image `out`. The map of the read is kept
/// through the decoding.
#[allow(clippy::too_many_arguments)]
pub fn flux_then_decode(
    mut read: ReadOpts,
    flux: PathBuf,
    out: &Path,
    drive: Drive,
    idx: usize,
    label: String,
    geometry: TrackSet,
) -> Vec<Task> {
    let _ = std::fs::remove_file(&flux);
    let _ = std::fs::remove_file(out);
    read.output = flux.clone();
    read.raw = true;
    let convert = Action::Convert(ConvertOpts {
        input: flux,
        output: out.to_path_buf(),
        format: read.format.clone(),
        tracks: read.tracks.clone(),
        ..Default::default()
    });
    vec![
        Task::Gw { action: Box::new(Action::Read(read)), drive, idx, label, geometry: geometry.clone() },
        Task::Expand(Box::new(|e: &mut Engine| {
            e.keep_map = true;
            Ok(Vec::new())
        })),
        Task::Gw { action: Box::new(convert), drive, idx, label: format!("DECODING DF{idx}"), geometry },
    ]
}

/// Re-read passes at most.
pub const PASSES: u32 = 3;

/// Above this many tracks without a single AmigaDOS sector, the disk is a game format or a
/// copy protection: those tracks are not re-read (NIBBLE copies them).
const MAX_NON_DOS: usize = 12;

/// Good / missing flags of each sector of the tracks still incomplete.
pub type Missing = BTreeMap<(u16, u8), Vec<bool>>;

/// Per-sector result of a track in a read map: gw's sector map when printed, else the state.
pub fn good_flags(info: &TrackInfo, spt: usize) -> Vec<bool> {
    let all = matches!(info.state, TrackState::Ok | TrackState::Recovered);
    (0..spt).map(|s| info.sector_map.get(s).copied().flatten().unwrap_or(all)).collect()
}

/// The bad tracks of a read, with what is already good on each. Tracks without any AmigaDOS
/// sector are left out when there are many of them (game format, not damage).
pub fn missing_sectors(map: &DiskMap, spt: usize) -> (Missing, usize) {
    let mut m = Missing::new();
    for (c, h) in map.bad_tracks() {
        m.insert((c, h), good_flags(&map.get(c, h), spt));
    }
    let non_dos: Vec<(u16, u8)> = m.iter().filter(|(_, f)| !f.contains(&true)).map(|(k, _)| *k).collect();
    if non_dos.len() > MAX_NON_DOS {
        for k in &non_dos {
            m.remove(k);
        }
        return (m, non_dos.len());
    }
    (m, 0)
}

/// Byte offset of a sector in an AmigaDOS image (cylinder-major, two heads).
fn offset(c: u16, h: u8, s: usize, spt: usize) -> usize {
    ((c as usize * 2 + h as usize) * spt + s) * BSIZE
}

/// Copy into `main` every sector still missing that the pass read correctly. Returns the
/// number of sectors gained; complete tracks leave `missing`.
pub fn merge(main: &mut [u8], pass: &[u8], pass_map: &DiskMap, missing: &mut Missing, spt: usize) -> usize {
    let mut gained = 0;
    for (&(c, h), flags) in missing.iter_mut() {
        let read = good_flags(&pass_map.get(c, h), spt);
        for (s, ok) in flags.iter_mut().enumerate() {
            if *ok || !read[s] {
                continue;
            }
            let o = offset(c, h, s, spt);
            if o + BSIZE <= main.len() && o + BSIZE <= pass.len() {
                main[o..o + BSIZE].copy_from_slice(&pass[o..o + BSIZE]);
                *ok = true;
                gained += 1;
            }
        }
    }
    missing.retain(|_, f| f.contains(&false));
    gained
}

/// Map entry of a re-read track after the merge.
pub fn merged_info(flags: Option<&Vec<bool>>, spt: usize) -> TrackInfo {
    let n = spt as u32;
    match flags {
        None => TrackInfo {
            state: TrackState::Recovered,
            sectors: Some((n, n)),
            retries: 1,
            sector_map: vec![Some(true); spt],
        },
        Some(f) => TrackInfo {
            state: TrackState::Bad,
            sectors: Some((f.iter().filter(|&&g| g).count() as u32, n)),
            retries: 1,
            sector_map: f.iter().map(|&g| Some(g)).collect(),
        },
    }
}

/// Everything a re-read pass needs.
pub struct Recovery {
    /// The image of the main read, completed in place.
    pub main: PathBuf,
    /// Image of one pass (deleted after the merge).
    pub temp: PathBuf,
    pub drive: Drive,
    pub idx: usize,
    pub format: String,
    pub hd: bool,
    pass: u32,
    missing: Missing,
    /// Raw flux of each pass kept here.
    keep: Option<Keep>,
    /// Tracks of the main read that went bad, for the final report.
    tracks: Vec<(u16, u8)>,
}

/// Step added after the main read: re-reads and merges the bad tracks, if any.
pub fn task(
    main: PathBuf,
    temp: PathBuf,
    drive: Drive,
    idx: usize,
    format: String,
    hd: bool,
    keep: Option<Keep>,
) -> Task {
    Task::Expand(Box::new(move |e: &mut Engine| {
        let spt = sectors_per_track(hd);
        let (missing, skipped) = missing_sectors(&e.map, spt);
        if skipped > 0 {
            e.push_log(
                format!(
                    "RE-READ: {skipped} tracks have no AmigaDOS sector (game format or protection): \
                     not re-read, copy this disk with NIBBLE."
                ),
                Level::Warn,
            );
        }
        if missing.is_empty() {
            if let Some(k) = &keep {
                e.push_log(format!("Flux of the read kept in {}", k.dir.display()), Level::Info);
            }
            return Ok(Vec::new());
        }
        let tracks: Vec<(u16, u8)> = missing.keys().copied().collect();
        let lost: usize = missing.values().map(|f| f.iter().filter(|&&g| !g).count()).sum();
        e.push_log(
            format!(
                "RE-READ: {} bad track(s), {lost} sector(s) missing: {}. Reading them again \
                 (5 revolutions, head re-seeks), up to {PASSES} passes.",
                tracks.len(),
                names(&tracks)
            ),
            Level::Warn,
        );
        let r = Recovery { main, temp, drive, idx, format, hd, pass: 0, missing, keep, tracks };
        Ok(next_pass(r, e))
    }))
}

fn names(t: &[(u16, u8)]) -> String {
    let v: Vec<String> = t.iter().take(12).map(|(c, h)| format!("{c}.{h}")).collect();
    v.join(" ") + if t.len() > 12 { " ..." } else { "" }
}

/// The next pass (gw read + merge), or nothing when done.
fn next_pass(mut r: Recovery, e: &mut Engine) -> Vec<Task> {
    if r.missing.is_empty() || r.pass >= PASSES {
        report(&r, e);
        return Vec::new();
    }
    r.pass += 1;
    let spt = sectors_per_track(r.hd);
    let keys: Vec<(u16, u8)> = r.missing.keys().copied().collect();
    let before = e.map.clone();
    let Some(spec) = e.map.start_reread_of(&keys) else { return Vec::new() };
    e.keep_map = true;
    let _ = std::fs::remove_file(&r.temp);
    let read = ReadOpts {
        output: r.temp.clone(),
        format: r.format.clone(),
        profile: ReadProfile::Recovery,
        tracks: TrackOpts { custom: spec, ..Default::default() },
        ..Default::default()
    };
    let cyls: Vec<u16> = keys.iter().map(|k| k.0).collect();
    let label = format!("RE-READING DF{} PASS {}/{PASSES}", r.idx, r.pass);
    let geometry = TrackSet { cyls, heads: vec![0, 1] };
    // The main read is pass 1 of the kept flux files.
    let steps = match &r.keep {
        Some(k) => flux_then_decode(read, k.pass(r.pass + 1), &r.temp, r.drive, r.idx, label, geometry),
        None => vec![Task::Gw {
            action: Box::new(Action::Read(read)),
            drive: r.drive,
            idx: r.idx,
            label,
            geometry,
        }],
    };
    let merge_step = Task::Expand(Box::new(move |e: &mut Engine| {
        let pass = std::fs::read(&r.temp).unwrap_or_default();
        let _ = std::fs::remove_file(&r.temp);
        let mut main = std::fs::read(&r.main).map_err(|err| format!("RE-READ: {err}"))?;
        let gained = merge(&mut main, &pass, &e.map, &mut r.missing, spt);
        if gained > 0 {
            std::fs::write(&r.main, &main).map_err(|err| format!("RE-READ: {err}"))?;
        }
        // The grid shows the whole disk again, with the merged result of the re-read tracks.
        let mut map = before;
        for &(c, h) in &keys {
            map.set(c, h, merged_info(r.missing.get(&(c, h)), spt));
        }
        e.map = map;
        let left: usize = r.missing.values().map(|f| f.iter().filter(|&&g| !g).count()).sum();
        e.push_log(
            format!("RE-READ PASS {}: {gained} sector(s) recovered, {left} still missing.", r.pass),
            if left == 0 { Level::Good } else { Level::Info },
        );
        if gained == 0 {
            // Nothing new this time: further passes would read the same thing.
            r.pass = PASSES;
        }
        Ok(next_pass(r, e))
    }));
    let mut v = steps;
    v.push(merge_step);
    v
}

fn report(r: &Recovery, e: &mut Engine) {
    if let Some(k) = &r.keep {
        e.push_log(format!("Flux of each pass kept in {}", k.dir.display()), Level::Info);
    }
    let fixed = r.tracks.len() - r.missing.len();
    if r.missing.is_empty() {
        e.push_log(format!("RE-READ: all {fixed} bad track(s) recovered."), Level::Good);
    } else {
        let left: Vec<(u16, u8)> = r.missing.keys().copied().collect();
        e.push_log(format!("RE-READ: {fixed} track(s) recovered, still bad: {}.", names(&left)), Level::Warn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hgf_core::gw::parse::{Output, TrackOutcome};

    const SPT: usize = 11;

    fn map_with(rows: &[((u16, u8), Vec<bool>)]) -> DiskMap {
        let mut m = DiskMap::new(hgf_core::MapMode::Read, TrackSet::dense(80, 2));
        for ((c, h), flags) in rows {
            let ok = flags.iter().filter(|&&g| g).count() as u32;
            m.apply(&Output::Track {
                cyl: *c,
                head: *h,
                outcome: TrackOutcome::Decoded { ok, total: SPT as u32, retry: false },
            });
            for (s, &g) in flags.iter().enumerate() {
                m.apply(&Output::SectorMapRow { head: *h, sector: s as u32, cells: vec![(*c, g)] });
            }
        }
        m.finish(true);
        m
    }

    fn flags(bad: &[usize]) -> Vec<bool> {
        (0..SPT).map(|s| !bad.contains(&s)).collect()
    }

    #[test]
    fn two_incomplete_reads_make_a_complete_track() {
        // pass 1 misses sectors 4-6, pass 2 misses 0-1: together everything was read once
        let main_map = map_with(&[((18, 1), flags(&[4, 5, 6]))]);
        let (mut missing, skipped) = missing_sectors(&main_map, SPT);
        assert_eq!(skipped, 0);
        assert_eq!(missing[&(18, 1)], flags(&[4, 5, 6]));

        let size = 901_120;
        let truth: Vec<u8> = (0..size).map(|i| (i / 512 % 251) as u8).collect();
        let mut main = truth.clone();
        for s in [4, 5, 6] {
            let o = offset(18, 1, s, SPT);
            main[o..o + 512].fill(0);
        }
        // pass 2 has the right bytes everywhere it says "good", zeros elsewhere
        let mut pass = truth.clone();
        for s in [0, 1] {
            let o = offset(18, 1, s, SPT);
            pass[o..o + 512].fill(0);
        }
        let pass_map = map_with(&[((18, 1), flags(&[0, 1]))]);
        assert_eq!(merge(&mut main, &pass, &pass_map, &mut missing, SPT), 3);
        assert!(missing.is_empty());
        assert_eq!(main, truth);
        assert_eq!(merged_info(None, SPT).state, TrackState::Recovered);
    }

    #[test]
    fn a_pass_that_reads_nothing_new_changes_nothing() {
        let m = map_with(&[((44, 0), flags(&[2]))]);
        let (mut missing, _) = missing_sectors(&m, SPT);
        let mut main = vec![1u8; 901_120];
        let pass = vec![9u8; 901_120];
        assert_eq!(merge(&mut main, &pass, &m, &mut missing, SPT), 0);
        assert!(main.iter().all(|&b| b == 1));
        let info = merged_info(missing.get(&(44, 0)), SPT);
        assert_eq!(info.state, TrackState::Bad);
        assert_eq!(info.sectors, Some((10, 11)));
    }

    #[test]
    fn game_disks_are_not_re_read() {
        let rows: Vec<((u16, u8), Vec<bool>)> =
            (2..20).map(|c| ((c, 0), vec![false; SPT])).chain([((40, 1), flags(&[3]))]).collect();
        let (missing, skipped) = missing_sectors(&map_with(&rows), SPT);
        assert_eq!(skipped, 18);
        assert_eq!(missing.keys().copied().collect::<Vec<_>>(), vec![(40, 1)]);
    }
}
