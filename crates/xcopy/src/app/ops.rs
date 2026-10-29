// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! The operations started by the buttons: copy, read to ADF, write ADF, format, erase,
//! check, directory, speed and cleaning, built as step lists for the [`Engine`].

use super::*;
use crate::engine::{Engine, Level, Task};
use crate::settings::*;
use hgf_core::disk::formats;
use hgf_core::disk::trackset::set_notation;
use hgf_core::fs::amigados::{self, kickstart, Filesystem, FormatOpts};
use hgf_core::gw::{Action, CleanOpts, EraseOpts, ReadOpts, ReadProfile, WriteOpts};
use hgf_core::TrackSet;
use std::path::{Path, PathBuf};

impl App {
    pub(super) fn notice(&mut self, title: &str, text: &str) {
        let lines = vec![(text.to_string(), Level::Warn)];
        self.modal = Some(Modal::new(ModalKind::Engine, title, lines, vec!["OK"]));
    }

    pub(super) fn dname(i: usize) -> String {
        format!("DF{i}")
    }

    pub(super) fn gw(&self, action: Action, idx: usize, label: String, geometry: TrackSet) -> Option<Task> {
        let drive = self.s.bus.drive(idx)?;
        Some(Task::Gw { action: Box::new(action), drive, idx, label, geometry })
    }

    pub(super) fn launch(&mut self, tasks: Vec<Task>) {
        self.eng.start(tasks, self.s.cfg.clone());
    }

    /// Tasks reading the source disk into `out` with the current copy mode. BAMCOPY first reads
    /// cylinder 40 (root block and bitmap), then only the cylinders holding allocated blocks.
    pub(super) fn read_tasks(&self, out: &Path) -> Vec<Task> {
        let src = self.s.source;
        let Some(drive) = self.s.bus.drive(src) else { return Vec::new() };
        let nibble = self.s.copy_mode == CopyMode::Nibble;
        let format = if nibble { String::new() } else { self.s.sector_format().to_string() };
        let (tracks, geo) = self.s.tracks();
        let profile = if self.s.copy_mode == CopyMode::DosCopyPlus {
            ReadProfile::Recovery
        } else {
            ReadProfile::Standard
        };
        let label = format!("READING {}", Self::dname(src));
        let hd = self.s.hd;
        let mut v = Vec::new();
        if self.s.copy_mode == CopyMode::BamCopy {
            let probe = temp_path("bam.adf");
            let _ = std::fs::remove_file(&probe);
            let probe_read = Action::Read(ReadOpts {
                output: probe.clone(),
                format: format.clone(),
                tracks: hgf_core::gw::TrackOpts { custom: "c=40:h=0-1".into(), ..Default::default() },
                ..Default::default()
            });
            v.push(Task::Gw {
                action: Box::new(probe_read),
                drive,
                idx: src,
                label: format!("READING BITMAP {}", Self::dname(src)),
                geometry: TrackSet { cyls: vec![40], heads: vec![0, 1] },
            });
            let out = out.to_path_buf();
            let (start, end) = (self.s.start, self.s.end);
            v.push(Task::Expand(Box::new(move |e: &mut Engine| {
                let data = std::fs::read(&probe).unwrap_or_default();
                let _ = std::fs::remove_file(&probe);
                let (tracks, geometry) = match amigados::used_cylinders(&data, hd) {
                    Ok(cyls) => {
                        let cyls: Vec<u16> = cyls.into_iter().filter(|c| (start..=end).contains(c)).collect();
                        e.push_log(
                            format!(
                                "BAMCOPY: {} cylinder(s) in use out of 80: {}",
                                cyls.len(),
                                set_notation(&cyls)
                            ),
                            Level::Good,
                        );
                        let t = hgf_core::gw::TrackOpts {
                            custom: format!("c={}:h=0-1", set_notation(&cyls)),
                            ..Default::default()
                        };
                        (t, TrackSet { cyls, heads: vec![0, 1] })
                    }
                    Err(err) => {
                        e.push_log(format!("BAMCOPY: {err} Reading the whole disk instead."), Level::Warn);
                        (tracks, geo)
                    }
                };
                let read = Action::Read(ReadOpts {
                    output: out.clone(),
                    format,
                    profile,
                    tracks,
                    ..Default::default()
                });
                let pad = out.clone();
                Ok(vec![
                    Task::Gw { action: Box::new(read), drive, idx: src, label, geometry },
                    Task::Expand(Box::new(move |_| amigados::pad_image(&pad, hd).map(|_| Vec::new()))),
                ])
            })));
        } else {
            let read = Action::Read(ReadOpts {
                output: out.to_path_buf(),
                format,
                profile,
                tracks,
                ..Default::default()
            });
            v.push(Task::Gw { action: Box::new(read), drive, idx: src, label, geometry: geo });
        }
        v
    }

    /// DF>ADF, once the destination file is chosen.
    pub(super) fn read_to_file(&mut self, out: PathBuf) {
        if self.eng.busy() || !self.source_ok() {
            return;
        }
        let _ = std::fs::remove_file(&out);
        let file = out.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
        let mut v = self.read_tasks(&out);
        if self.s.copy_mode != CopyMode::Nibble {
            v.push(Task::BootCheck { path: out.clone(), hd: self.s.hd });
        }
        v.push(Task::Done { message: format!("IMAGE SAVED: {file}") });
        self.launch(v);
    }

    /// ADF>DF, once the image is chosen.
    pub(super) fn write_from_file(&mut self, input: PathBuf) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        let Some(targets) = self.targets_or_notice() else { return };
        let (tracks, geo) = self.s.tracks();
        let size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
        let ext_in = formats::extension(&input).unwrap_or_default();
        let format = match formats::guess_format(&input, size) {
            Some(f) => f.to_string(),
            None if ext_in == "adf" => {
                return self.notice(
                    "BAD IMAGE",
                    &format!("THIS ADF IS {size} BYTES. EXPECTED 901120 (DD) OR 1802240 (HD)."),
                )
            }
            None => String::new(),
        };
        let geo = if format.is_empty() { TrackSet::dense(80, 2) } else { geo };
        let tracks = if format.is_empty() { Default::default() } else { tracks };
        let img = LastImage { path: input, format, tracks, geo };
        let mut v = vec![Self::destroy_confirm(&targets)];
        v.extend(self.write_tasks(&img, &targets, None, false));
        v.push(Task::Done { message: "OPERATION DONE. IMAGE WRITTEN.".into() });
        self.pending_image = Some((self.eng.done_count + 1, img));
        self.launch(v);
    }

    pub(super) fn write_task(
        &self,
        input: &Path,
        format: &str,
        tracks: hgf_core::gw::TrackOpts,
        no_verify: bool,
        idx: usize,
        geo: TrackSet,
    ) -> Option<Task> {
        let action = Action::Write(WriteOpts {
            input: input.to_path_buf(),
            format: format.to_string(),
            tracks,
            no_verify,
            ..Default::default()
        });
        self.gw(action, idx, format!("WRITING {}", Self::dname(idx)), geo)
    }

    pub(super) fn destroy_confirm(targets: &[usize]) -> Task {
        let list: Vec<String> = targets.iter().map(|&t| Self::dname(t)).collect();
        Task::Confirm {
            title: "WARNING!".into(),
            text: format!("ALL DATA ON {} WILL BE DESTROYED. CONTINUE?", list.join(", ")),
        }
    }

    pub(super) fn source_ok(&mut self) -> bool {
        if self.s.bus.drive(self.s.source).is_none() {
            self.notice("NO SOURCE DRIVE", "SELECT A SOURCE DRIVE AVAILABLE ON THIS BUS (SEE SETUP).");
            return false;
        }
        true
    }

    pub(super) fn targets_or_notice(&mut self) -> Option<Vec<usize>> {
        let t = self.s.target_list();
        if t.is_empty() {
            self.notice(
                "NO TARGET DRIVE",
                "CLICK A LAMP UNDER THE DRIVES TO SELECT AT LEAST ONE TARGET DRIVE.",
            );
            return None;
        }
        Some(t)
    }

    /// Writes of `image` to every target; the source drive (disk swap) goes last with a prompt.
    pub(super) fn write_tasks(
        &self,
        img: &LastImage,
        targets: &[usize],
        swap_from: Option<usize>,
        no_verify: bool,
    ) -> Vec<Task> {
        let mut order: Vec<usize> = targets.iter().copied().filter(|&t| Some(t) != swap_from).collect();
        if let Some(s) = swap_from.filter(|s| targets.contains(s)) {
            order.push(s);
        }
        let mut v = Vec::new();
        for t in order {
            if Some(t) == swap_from {
                v.push(Task::Prompt {
                    title: "CHANGE DISK".into(),
                    text: format!("REMOVE THE SOURCE DISK AND INSERT THE TARGET DISK IN {}.", Self::dname(t)),
                });
            }
            v.extend(self.write_task(
                &img.path,
                &img.format,
                img.tracks.clone(),
                no_verify,
                t,
                img.geo.clone(),
            ));
        }
        v
    }

    pub(super) fn start(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        match self.s.section {
            Section::Copy => self.start_copy(),
            Section::Tools => self.start_tool(),
        }
    }

    pub(super) fn start_copy(&mut self) {
        let nibble = self.s.copy_mode == CopyMode::Nibble;
        let format = if nibble { String::new() } else { self.s.sector_format().to_string() };
        let ext = if nibble { "scp" } else { "adf" };
        let (tracks, geo) = self.s.tracks();
        let src = self.s.source;
        match self.s.flow {
            Flow::DiskToDisk => {
                if !self.source_ok() {
                    return;
                }
                let Some(targets) = self.targets_or_notice() else { return };
                let temp = temp_path(&format!("copy.{ext}"));
                let _ = std::fs::remove_file(&temp);
                self.temps.push(temp.clone());
                // BAMCOPY writes the whole disk (unread tracks blank), so the target is clean.
                let (tracks, geo) = if self.s.copy_mode == CopyMode::BamCopy {
                    (Default::default(), TrackSet::dense(80, 2))
                } else {
                    (tracks, geo)
                };
                let mut v = vec![Self::destroy_confirm(&targets)];
                v.extend(self.read_tasks(&temp));
                v.push(Task::CheckBad);
                if !nibble {
                    v.push(Task::BootCheck { path: temp.clone(), hd: self.s.hd });
                }
                let img = LastImage { path: temp, format, tracks, geo };
                v.extend(self.write_tasks(&img, &targets, Some(src), false));
                v.push(Task::Done {
                    message: format!(
                        "OPERATION DONE. {} COP{} WRITTEN.",
                        targets.len(),
                        if targets.len() > 1 { "IES" } else { "Y" }
                    ),
                });
                self.pending_image = Some((self.eng.done_count + 1, img));
                self.launch(v);
            }
            Flow::DiskToAdf => {
                if !self.source_ok() {
                    return;
                }
                // Read cylinder 40 first: the volume name becomes the default file name.
                let probe = temp_path("probe.adf");
                let _ = std::fs::remove_file(&probe);
                self.temps.push(probe.clone());
                let read = Action::Read(ReadOpts {
                    output: probe.clone(),
                    format: self.s.sector_format().into(),
                    profile: ReadProfile::QuickScan,
                    tracks: hgf_core::gw::TrackOpts { custom: "c=40:h=0-1".into(), ..Default::default() },
                    ..Default::default()
                });
                let geo40 = TrackSet { cyls: vec![40], heads: vec![0, 1] };
                let mut v: Vec<Task> = self
                    .gw(read, src, format!("READING NAME {}", Self::dname(src)), geo40)
                    .into_iter()
                    .collect();
                v.push(Task::NameProbe { path: probe, hd: self.s.hd });
                self.launch(v);
            }
            Flow::AdfToDisk => {
                if self.targets_or_notice().is_some() {
                    self.spawn_dialog(Pick::WriteFrom, None);
                }
            }
        }
    }

    pub(super) fn start_tool(&mut self) {
        let (tracks, geo) = self.s.tracks();
        match self.s.tool_mode {
            ToolMode::Format | ToolMode::QFormat => {
                let opts = FormatOpts {
                    name: self.s.volume.clone(),
                    filesystem: if self.s.ffs { Filesystem::Ffs } else { Filesystem::Ofs },
                    international: false,
                    hd: self.s.hd,
                };
                let image = match amigados::blank_image(&opts) {
                    Ok(i) => i,
                    Err(e) => return self.notice("BAD VOLUME NAME", &e),
                };
                if self.s.tool_adf {
                    let name = format!("{}.adf", self.s.volume.trim().replace(' ', "_"));
                    self.spawn_dialog(Pick::SaveBlank(image), Some((name, "adf", "Amiga disk file")));
                    return;
                }
                let Some(targets) = self.targets_or_notice() else { return };
                let temp = temp_path("format.adf");
                if let Err(e) = std::fs::write(&temp, &image) {
                    return self.notice("TEMP FILE", &e.to_string());
                }
                self.temps.push(temp.clone());
                let img = LastImage {
                    path: temp,
                    format: opts.gw_format().into(),
                    tracks: Default::default(),
                    geo: TrackSet::dense(80, 2),
                };
                let quick = self.s.tool_mode == ToolMode::QFormat;
                let mut v = vec![Self::destroy_confirm(&targets)];
                for t in &targets {
                    let mut task = self.write_task(
                        &img.path,
                        &img.format,
                        Default::default(),
                        quick,
                        *t,
                        img.geo.clone(),
                    );
                    if let Some(Task::Gw { label, .. }) = &mut task {
                        *label = format!("FORMATTING {}", Self::dname(*t));
                    }
                    v.extend(task);
                }
                v.push(Task::Done {
                    message: format!(
                        "FORMAT DONE: \"{}\" {}.",
                        self.s.volume.to_uppercase(),
                        if self.s.ffs { "FFS" } else { "OFS" }
                    ),
                });
                self.launch(v);
            }
            ToolMode::Track0 => {
                let mut opts = FormatOpts {
                    name: self.s.volume.clone(),
                    filesystem: if self.s.ffs { Filesystem::Ffs } else { Filesystem::Ofs },
                    international: false,
                    hd: self.s.hd,
                };
                if amigados::validate_name(&opts.name).is_err() {
                    opts.name = "Empty".into();
                }
                let image = match amigados::blank_image(&opts) {
                    Ok(i) => i,
                    Err(e) => return self.notice("TRACK 0", &e),
                };
                let Some(targets) = self.targets_or_notice() else { return };
                let temp = temp_path("track0.adf");
                if let Err(e) = std::fs::write(&temp, &image) {
                    return self.notice("TEMP FILE", &e.to_string());
                }
                self.temps.push(temp.clone());
                let list: Vec<String> = targets.iter().map(|&t| Self::dname(t)).collect();
                let mut v = vec![Task::Confirm {
                    title: "WARNING!".into(),
                    text: format!(
                        "TRACK 0 (BOOT BLOCK) OF {} WILL BE ERASED AND REWRITTEN. THE DISK WILL NO LONGER BOOT. CONTINUE?",
                        list.join(", ")
                    ),
                }];
                for &t in &targets {
                    let action = Action::Write(WriteOpts {
                        input: temp.clone(),
                        format: opts.gw_format().into(),
                        tracks: hgf_core::gw::TrackOpts { custom: "c=0:h=0-1".into(), ..Default::default() },
                        pre_erase: true,
                        retries: Some(8),
                        ..Default::default()
                    });
                    let geo = TrackSet { cyls: vec![0], heads: vec![0, 1] };
                    v.extend(self.gw(action, t, format!("REWRITING TRACK 0 {}", Self::dname(t)), geo));
                }
                v.push(Task::Done { message: "TRACK 0 REWRITTEN AND VERIFIED.".into() });
                self.launch(v);
            }
            ToolMode::KickDisk => self.spawn_dialog(Pick::KickRom, None),
            ToolMode::Erase => {
                let Some(targets) = self.targets_or_notice() else { return };
                let mut v = vec![Self::destroy_confirm(&targets)];
                for &t in &targets {
                    let a = Action::Erase(EraseOpts { tracks: tracks.clone(), ..Default::default() });
                    v.extend(self.gw(a, t, format!("ERASING {}", Self::dname(t)), geo.clone()));
                }
                v.push(Task::Done { message: "ERASE DONE.".into() });
                self.launch(v);
            }
            ToolMode::Speed => {
                if !self.source_ok() {
                    return;
                }
                let src = self.s.source;
                let mut v: Vec<Task> = self
                    .gw(Action::Rpm { nr: 10 }, src, format!("SPEED CHECK {}", Self::dname(src)), geo)
                    .into_iter()
                    .collect();
                v.push(Task::RpmDone);
                self.launch(v);
            }
            ToolMode::Clean => {
                let Some(targets) = self.targets_or_notice() else { return };
                let t = targets[0];
                let mut v = vec![Task::Prompt {
                    title: "HEAD CLEANING".into(),
                    text: format!("INSERT A CLEANING DISK (WET IT WITH ISOPROPYL) IN {}.", Self::dname(t)),
                }];
                v.extend(self.gw(
                    Action::Clean(CleanOpts::default()),
                    t,
                    format!("CLEANING {}", Self::dname(t)),
                    geo,
                ));
                v.push(Task::Done { message: "CLEANING DONE.".into() });
                self.launch(v);
            }
        }
    }

    pub(super) fn repeat(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        let rewrite = self.s.section == Section::Copy && self.s.flow != Flow::DiskToAdf;
        if let (true, Some(img)) = (rewrite, self.last_image.clone()) {
            if !img.path.exists() {
                self.last_image = None;
                return self.start();
            }
            let Some(targets) = self.targets_or_notice() else { return };
            let list: Vec<String> = targets.iter().map(|&t| Self::dname(t)).collect();
            let mut v = vec![Task::Prompt {
                title: "REPEAT".into(),
                text: format!(
                    "INSERT NEW TARGET DISK(S) IN {}. THE SAME IMAGE WILL BE WRITTEN AGAIN.",
                    list.join(", ")
                ),
            }];
            v.extend(self.write_tasks(&img, &targets, None, false));
            v.push(Task::Done { message: "OPERATION DONE. ONE MORE COPY.".into() });
            self.launch(v);
        } else {
            self.start();
        }
    }

    pub(super) fn check_disk(&mut self) {
        if self.eng.busy() || self.picking.is_some() || !self.source_ok() {
            return;
        }
        let (tracks, geo) = self.s.tracks();
        let temp = temp_path("check.adf");
        self.temps.push(temp.clone());
        let read = Action::Read(ReadOpts {
            output: temp,
            format: self.s.sector_format().into(),
            profile: ReadProfile::QuickScan,
            tracks,
            ..Default::default()
        });
        let src = self.s.source;
        let mut v: Vec<Task> =
            self.gw(read, src, format!("CHECKING {}", Self::dname(src)), geo).into_iter().collect();
        v.push(Task::CheckDone);
        self.launch(v);
    }

    pub(super) fn directory(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        let from_file = (self.s.section == Section::Copy && self.s.flow == Flow::AdfToDisk)
            || (self.s.section == Section::Tools && self.s.tool_mode.is_format() && self.s.tool_adf);
        if from_file {
            self.spawn_dialog(Pick::Dir, None);
            return;
        }
        if !self.source_ok() {
            return;
        }
        let temp = temp_path("dir.adf");
        let _ = std::fs::remove_file(&temp);
        self.temps.push(temp.clone());
        // Root block and directory blocks can be anywhere: read the whole disk, quickly.
        let read = Action::Read(ReadOpts {
            output: temp.clone(),
            format: self.s.sector_format().into(),
            profile: ReadProfile::QuickScan,
            ..Default::default()
        });
        let src = self.s.source;
        let mut v: Vec<Task> = self
            .gw(read, src, format!("READING {}", Self::dname(src)), TrackSet::dense(80, 2))
            .into_iter()
            .collect();
        v.push(Task::Dir { path: temp });
        v.push(Task::Done { message: "OPERATION DONE.".into() });
        self.launch(v);
    }

    pub(super) fn disk_info(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        let idx = if self.s.bus.drive(self.s.source).is_some() { self.s.source } else { 0 };
        let mut v: Vec<Task> = self
            .gw(
                Action::Info { bootloader: false },
                idx,
                "PROBING GREASEWEAZLE".into(),
                TrackSet::dense(80, 2),
            )
            .into_iter()
            .collect();
        v.push(Task::Info);
        self.launch(v);
    }

    // ───────────────────────────── requesters ─────────────────────────────

    /// KICKDISK, once the ROM image is chosen: an Amiga 1000 Kickstart disk (block 0 `KICK`,
    /// blocks 1-512 the 256K ROM), written on cylinders 0-23 or saved as an ADF.
    pub(super) fn kick_disk_from(&mut self, rom_path: PathBuf) {
        if self.eng.busy() {
            return;
        }
        let file = match std::fs::read(&rom_path) {
            Ok(f) => f,
            Err(e) => return self.notice("ROM IMAGE", &format!("Cannot read the file: {e}")),
        };
        // Amiga Forever encrypted ROMs come with a rom.key in the same folder.
        let key = rom_path.parent().and_then(|d| std::fs::read(d.join("rom.key")).ok());
        let rom = match kickstart::load_rom(&file, key.as_deref()) {
            Ok(r) => r,
            Err(e) => return self.notice("ROM IMAGE", &e),
        };
        for n in &rom.notes {
            self.eng.push_log(format!("Kickstart ROM: {n}"), Level::Info);
        }
        let what = rom.describe().to_uppercase();
        if !rom.checksum_ok {
            self.eng.push_log("Kickstart ROM checksum is wrong: the image is probably damaged.", Level::Warn);
        }
        let image = kickstart::kick_disk_image(&rom);
        if self.s.tool_adf {
            let name = format!("kick{}{}.adf", rom.version, rom.revision);
            self.spawn_dialog(Pick::SaveBlank(image), Some((name, "adf", "Amiga disk file")));
            return;
        }
        let Some(targets) = self.targets_or_notice() else { return };
        let temp = temp_path("kick.adf");
        if let Err(e) = std::fs::write(&temp, &image) {
            return self.notice("TEMP FILE", &e.to_string());
        }
        self.temps.push(temp.clone());
        let list: Vec<String> = targets.iter().map(|&t| Self::dname(t)).collect();
        let warn = if rom.checksum_ok { "" } else { " WARNING: THE ROM CHECKSUM IS WRONG (DAMAGED IMAGE?)." };
        let mut v = vec![Task::Confirm {
            title: "KICKSTART DISK".into(),
            text: format!(
                "{what} WILL BE WRITTEN ON {} (CYLINDERS 0-{}): ITS CONTENT THERE IS LOST.{warn} CONTINUE?",
                list.join(", "),
                kickstart::KICK_CYLINDERS - 1
            ),
        }];
        let tracks = hgf_core::gw::TrackOpts {
            custom: format!("c=0-{}:h=0-1", kickstart::KICK_CYLINDERS - 1),
            ..Default::default()
        };
        let geo = TrackSet::dense(kickstart::KICK_CYLINDERS, 2);
        for &t in &targets {
            let mut task = self.write_task(&temp, "amiga.amigados", tracks.clone(), false, t, geo.clone());
            if let Some(Task::Gw { label, .. }) = &mut task {
                *label = format!("WRITING KICKSTART {}", Self::dname(t));
            }
            v.extend(task);
        }
        v.push(Task::Done { message: format!("KICKSTART DISK READY: {what}.") });
        self.launch(v);
    }
}
