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
    /// `name`: image file shown in the status line (empty for internal images).
    pub(super) fn read_tasks(&self, out: &Path, name: &str) -> Vec<Task> {
        let src = self.s.source;
        let Some(drive) = self.s.bus.drive(src) else { return Vec::new() };
        let nibble = self.s.copy_mode == CopyMode::Nibble;
        let format = if nibble { String::new() } else { self.s.sector_format().to_string() };
        let (tracks, geo) = if nibble { self.s.nibble_tracks() } else { self.s.tracks() };
        let profile = if self.s.copy_mode == CopyMode::DosCopyPlus {
            ReadProfile::Recovery
        } else {
            ReadProfile::Standard
        };
        let keep = self.keep_flux(out, name);
        let label = if name.is_empty() {
            format!("READING {}", Self::dname(src))
        } else {
            format!("READING {} > {name}", Self::dname(src))
        };
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
            let read = ReadOpts { output: out.to_path_buf(), format, profile, tracks, ..Default::default() };
            match &keep {
                // Flux kept: pass 1 is read raw, then decoded into the image.
                Some(k) => {
                    v.extend(crate::recover::flux_then_decode(read, k.pass(1), out, drive, src, label, geo))
                }
                None => v.push(Task::Gw {
                    action: Box::new(Action::Read(read)),
                    drive,
                    idx: src,
                    label,
                    geometry: geo,
                }),
            }
        }
        // Bad tracks: read them again and merge the sectors recovered (not for flux copies).
        if !nibble && self.s.reread {
            let temp = temp_path("reread.adf");
            let fmt = self.s.sector_format().to_string();
            v.push(crate::recover::task(out.to_path_buf(), temp, drive, src, fmt, hd, keep));
        }
        v
    }

    /// SETUP "KEEP FLUX OF EACH PASS": where the passes of this read go. Beside the image for
    /// DF>ADF; for a disk-to-disk copy (temporary image), in the last folder used.
    fn keep_flux(&self, out: &Path, name: &str) -> Option<crate::recover::Keep> {
        if !self.s.keep_flux || self.s.copy_mode == CopyMode::Nibble {
            return None;
        }
        let (dir, stem) = if name.is_empty() {
            let home =
                std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
            let dir = self.start_dir().or(home).unwrap_or_else(|| PathBuf::from("."));
            (dir, format!("DF{} copy {}", self.s.source, hgf_core::fs::bytes::now_secs()))
        } else {
            let stem =
                out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "disk".into());
            (out.parent().map(Path::to_path_buf).unwrap_or_default(), stem)
        };
        crate::recover::Keep::new(&dir, &stem).ok()
    }

    /// DF>ADF, once the destination file is chosen.
    pub(super) fn read_to_file(&mut self, out: PathBuf) {
        if self.eng.busy() || !self.source_ok() {
            return;
        }
        let _ = std::fs::remove_file(&out);
        let file = out.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
        let shown = short_name(&out, 20);
        let mut v = self.read_tasks(&out, &shown);
        if self.s.copy_mode != CopyMode::Nibble {
            v.push(Task::BootCheck { path: out.clone(), hd: self.s.hd });
        }
        v.push(Task::Done { message: format!("IMAGE SAVED: {file}") });
        let ext = if self.s.copy_mode == CopyMode::Nibble { "scp" } else { "adf" };
        self.last_op = Some((
            OpKey::Copy(Flow::DiskToAdf),
            Redo::ReadTo { dir: out.parent().map(Path::to_path_buf), ext },
        ));
        self.image_name = Some((Flow::DiskToAdf, file));
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
        let name = short_name(&input, 20);
        let img = LastImage { path: input, format, tracks, geo, name: name.clone() };
        let job = WriteJob {
            img,
            verb: "WRITING",
            no_verify: false,
            pre_erase: false,
            retries: None,
            hfreq_erase: false,
        };
        let done = format!("OPERATION DONE. {name} WRITTEN.");
        let mut v = self.confirm_first(Self::destroy_confirm(&targets));
        v.extend(self.write_tasks(&job, &targets, None));
        v.push(Task::Done { message: done.clone() });
        // REPEAT writes this image again, without the file requester.
        self.last_op = Some((OpKey::Copy(Flow::AdfToDisk), Redo::Write { job, done }));
        self.image_name = Some((Flow::AdfToDisk, name));
        self.launch(v);
    }

    /// One write of `job` on drive `idx`; the label names the image file when there is one.
    pub(super) fn write_task(&self, job: &WriteJob, idx: usize) -> Option<Task> {
        // Amiga trackdisk precompensation on the inner cylinders of DD disks (flux images
        // already hold their timings, HD disks are written without it).
        let precomp = if self.s.write_precomp && job.img.format == "amiga.amigados" {
            hgf_core::diag::compat::AMIGA_PRECOMP.to_string()
        } else {
            String::new()
        };
        let action = Action::Write(WriteOpts {
            input: job.img.path.clone(),
            format: job.img.format.clone(),
            tracks: job.img.tracks.clone(),
            no_verify: job.no_verify,
            pre_erase: job.pre_erase,
            retries: job.retries,
            precomp,
            ..Default::default()
        });
        let label = if job.img.name.is_empty() {
            format!("{} {}", job.verb, Self::dname(idx))
        } else {
            format!("{} {} < {}", job.verb, Self::dname(idx), job.img.name)
        };
        self.gw(action, idx, label, job.img.geo.clone())
    }

    /// The confirmation step, unless SETUP turned confirmations off.
    pub(super) fn confirm_first(&self, confirm: Task) -> Vec<Task> {
        if self.s.confirm {
            vec![confirm]
        } else {
            Vec::new()
        }
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
        job: &WriteJob,
        targets: &[usize],
        swap_from: Option<usize>,
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
            if job.hfreq_erase {
                // A high-frequency erase removes old signals that a plain erase leaves behind.
                let a = Action::Erase(EraseOpts {
                    tracks: job.img.tracks.clone(),
                    hfreq: true,
                    ..Default::default()
                });
                v.extend(self.gw(a, t, format!("HF ERASE {}", Self::dname(t)), job.img.geo.clone()));
            }
            v.extend(self.write_task(job, t));
        }
        v
    }

    pub(super) fn start(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        self.auto_save = None;
        match self.s.section {
            Section::Copy => self.start_copy(),
            Section::Tools => self.start_tool(),
        }
    }

    pub(super) fn start_copy(&mut self) {
        let nibble = self.s.copy_mode == CopyMode::Nibble;
        let format = if nibble { String::new() } else { self.s.sector_format().to_string() };
        let ext = if nibble { "scp" } else { "adf" };
        let (tracks, geo) = if nibble { self.s.nibble_tracks() } else { self.s.tracks() };
        let src = self.s.source;
        match self.s.flow {
            Flow::DiskToDisk => {
                if !self.source_ok() {
                    return;
                }
                let Some(targets) = self.targets_or_notice() else { return };
                let temp = temp_path(&format!("copy.{ext}"));
                // the previous copy image is overwritten: REPEAT waits for the new one
                if matches!(self.last_op, Some((OpKey::Copy(Flow::DiskToDisk), _))) {
                    self.last_op = None;
                }
                let _ = std::fs::remove_file(&temp);
                self.temps.push(temp.clone());
                // BAMCOPY writes the whole disk (unread tracks blank), so the target is clean.
                let (tracks, geo) = if self.s.copy_mode == CopyMode::BamCopy {
                    (Default::default(), TrackSet::dense(80, 2))
                } else {
                    (tracks, geo)
                };
                let mut v = self.confirm_first(Self::destroy_confirm(&targets));
                v.extend(self.read_tasks(&temp, ""));
                v.push(Task::CheckBad);
                if !nibble {
                    v.push(Task::BootCheck { path: temp.clone(), hd: self.s.hd });
                }
                let img = LastImage { path: temp, format, tracks, geo, name: String::new() };
                let job = WriteJob {
                    img,
                    verb: "WRITING",
                    no_verify: false,
                    pre_erase: false,
                    retries: None,
                    hfreq_erase: false,
                };
                v.extend(self.write_tasks(&job, &targets, Some(src)));
                v.push(Task::Done {
                    message: format!(
                        "OPERATION DONE. {} COP{} WRITTEN.",
                        targets.len(),
                        if targets.len() > 1 { "IES" } else { "Y" }
                    ),
                });
                // The image exists only once read: REPEAT gets it when the copy completes.
                let redo = Redo::Write { job, done: "OPERATION DONE. ONE MORE COPY.".into() };
                self.pending_op = Some((self.eng.done_count + 1, (OpKey::Copy(Flow::DiskToDisk), redo)));
                self.image_name = None;
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
                let mut v: Vec<Task> = Vec::new();
                if self.auto_save.is_some() {
                    v.push(Task::Prompt {
                        title: "REPEAT".into(),
                        text: format!("INSERT THE NEXT SOURCE DISK IN {}.", Self::dname(src)),
                    });
                }
                v.extend(self.gw(read, src, format!("READING NAME {}", Self::dname(src)), geo40));
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
                    name: String::new(),
                };
                let quick = self.s.tool_mode == ToolMode::QFormat;
                let job = WriteJob {
                    img,
                    verb: "FORMATTING",
                    no_verify: quick,
                    pre_erase: false,
                    retries: None,
                    hfreq_erase: false,
                };
                let done = format!(
                    "FORMAT DONE: \"{}\" {}.",
                    self.s.volume.to_uppercase(),
                    if self.s.ffs { "FFS" } else { "OFS" }
                );
                let mut v = self.confirm_first(Self::destroy_confirm(&targets));
                v.extend(self.write_tasks(&job, &targets, None));
                v.push(Task::Done { message: done.clone() });
                self.last_op = Some((OpKey::Tool(self.s.tool_mode), Redo::Write { job, done }));
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
                let mut v = self.confirm_first(Task::Confirm {
                    title: "WARNING!".into(),
                    text: format!(
                        "TRACK 0 (BOOT BLOCK) OF {} WILL BE ERASED AND REWRITTEN. THE DISK WILL NO LONGER BOOT. CONTINUE?",
                        list.join(", ")
                    ),
                });
                let img = LastImage {
                    path: temp,
                    format: opts.gw_format().into(),
                    tracks: hgf_core::gw::TrackOpts { custom: "c=0:h=0-1".into(), ..Default::default() },
                    geo: TrackSet { cyls: vec![0], heads: vec![0, 1] },
                    name: String::new(),
                };
                let job = WriteJob {
                    img,
                    verb: "REWRITING TRACK 0",
                    no_verify: false,
                    pre_erase: true,
                    retries: Some(8),
                    hfreq_erase: true,
                };
                let done = "TRACK 0 REWRITTEN AND VERIFIED.".to_string();
                v.extend(self.write_tasks(&job, &targets, None));
                v.push(Task::Done { message: done.clone() });
                self.last_op = Some((OpKey::Tool(ToolMode::Track0), Redo::Write { job, done }));
                self.launch(v);
                self.eng.fail_hint = self.track0_hint();
            }
            ToolMode::KickDisk => self.spawn_dialog(Pick::KickRom, None),
            ToolMode::Erase => {
                let Some(targets) = self.targets_or_notice() else { return };
                let mut v = self.confirm_first(Self::destroy_confirm(&targets));
                v.extend(self.erase_tasks(&tracks, &geo, &targets));
                v.push(Task::Done { message: "ERASE DONE.".into() });
                self.last_op = Some((OpKey::Tool(ToolMode::Erase), Redo::Erase { tracks, geo }));
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

    pub(super) fn erase_tasks(
        &self,
        tracks: &hgf_core::gw::TrackOpts,
        geo: &TrackSet,
        targets: &[usize],
    ) -> Vec<Task> {
        let mut v = Vec::new();
        for &t in targets {
            let a = Action::Erase(EraseOpts { tracks: tracks.clone(), ..Default::default() });
            v.extend(self.gw(a, t, format!("ERASING {}", Self::dname(t)), geo.clone()));
        }
        v
    }

    /// Shown when TRACK0 fails anyway: what is left to tell a damaged disk from a drive problem.
    pub(super) fn track0_hint(&self) -> Vec<(String, Level)> {
        let other = match self.s.target_list().first() {
            Some(0) if self.s.bus.drive(1).is_some() => "DF1",
            Some(_) if self.s.bus.drive(0).is_some() => "DF0",
            _ => "the other drive",
        };
        [
            "Still failing after HF erase and 8 tries:".to_string(),
            "- Clean the heads (TOOLS > CLEAN).".to_string(),
            format!("- Try the disk in {other}. Fails too: track 0 is damaged (no boot, data disk only)."),
            format!("- Works in {other}: this drive is dirty or misaligned."),
            "- HD disk taped as DD? Use a real DD disk.".to_string(),
            "- Stubborn old signal: use a bulk eraser.".to_string(),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, if i == 0 { Level::Warn } else { Level::Info }))
        .collect()
    }

    /// The command selected on screen, as REPEAT sees it (`None`: nothing to replay).
    pub(super) fn current_key(&self) -> Option<OpKey> {
        match self.s.section {
            Section::Copy => Some(OpKey::Copy(self.s.flow)),
            Section::Tools => {
                let m = self.s.tool_mode;
                let to_file = (m.is_format() || m == ToolMode::KickDisk) && self.s.tool_adf;
                (!to_file).then_some(OpKey::Tool(m))
            }
        }
    }

    /// "INSERT NEW DISK(S)" before a repeated write or erase (with the warning when SETUP asks
    /// for confirmations).
    fn repeat_prompt(&self, targets: &[usize], what: &str) -> Task {
        let list: Vec<String> = targets.iter().map(|&t| Self::dname(t)).collect();
        let warn = if self.s.confirm { " ALL DATA ON THEM WILL BE DESTROYED." } else { "" };
        Task::Prompt {
            title: "REPEAT".into(),
            text: format!("INSERT NEW DISK(S) IN {}. {what}{warn}", list.join(", ")),
        }
    }

    /// REPEAT (F2): run the last COPY / FORMAT / ERASE… again on new disks, without reading the
    /// source again and without asking for the image file.
    pub(super) fn repeat(&mut self) {
        if self.eng.busy() || self.picking.is_some() {
            return;
        }
        let key = self.current_key();
        let redo = self.last_op.as_ref().filter(|(k, _)| Some(*k) == key).map(|(_, r)| r.clone());
        match redo {
            Some(Redo::Write { job, done }) if job.img.path.exists() => {
                let Some(targets) = self.targets_or_notice() else { return };
                let what = if job.img.name.is_empty() {
                    format!("REPEAT: {}.", job.verb)
                } else {
                    format!("{} WILL BE WRITTEN AGAIN.", job.img.name)
                };
                self.eng.push_log(format!("REPEAT: {} ON {} DISK(S).", job.verb, targets.len()), Level::Info);
                let mut v = vec![self.repeat_prompt(&targets, &what)];
                v.extend(self.write_tasks(&job, &targets, None));
                v.push(Task::Done { message: done });
                if key == Some(OpKey::Copy(Flow::AdfToDisk)) {
                    self.image_name = Some((Flow::AdfToDisk, job.img.name.clone()));
                }
                let hint = if job.hfreq_erase { self.track0_hint() } else { Vec::new() };
                self.launch(v);
                self.eng.fail_hint = hint;
            }
            Some(Redo::Erase { tracks, geo }) => {
                let Some(targets) = self.targets_or_notice() else { return };
                self.eng.push_log(format!("REPEAT: ERASE ON {} DISK(S).", targets.len()), Level::Info);
                let mut v = vec![self.repeat_prompt(&targets, "THEY WILL BE ERASED.")];
                v.extend(self.erase_tasks(&tracks, &geo, &targets));
                v.push(Task::Done { message: "ERASE DONE.".into() });
                self.launch(v);
            }
            Some(Redo::ReadTo { dir, ext }) => {
                if !self.source_ok() {
                    return;
                }
                let folder = dir.as_deref().map(|d| d.display().to_string()).unwrap_or_default();
                self.eng.push_log(
                    format!("REPEAT: READ THE NEXT DISK, SAVED AS <VOLUME NAME>.{ext} IN {folder}"),
                    Level::Info,
                );
                // the volume name probe then saves without the requester
                self.auto_save = Some(dir);
                self.start_copy();
            }
            _ => {
                let what = match self.s.section {
                    Section::Tools => format!("REPEAT: {} AGAIN.", self.s.tool_mode.label()),
                    Section::Copy => match self.s.flow {
                        Flow::DiskToAdf => "REPEAT: READ ANOTHER DISK.".to_string(),
                        Flow::AdfToDisk => "REPEAT: NO IMAGE WRITTEN YET, CHOOSE ONE.".to_string(),
                        Flow::DiskToDisk => "REPEAT: NO COPY IN MEMORY YET, FULL COPY STARTED.".to_string(),
                    },
                };
                self.eng.push_log(what.clone(), Level::Info);
                self.start();
                if !self.eng.busy() && self.picking.is_none() && self.modal.is_none() {
                    self.eng.set_status(what, Level::Warn);
                }
            }
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
        let mut v = self.confirm_first(Task::Confirm {
            title: "KICKSTART DISK".into(),
            text: format!(
                "{what} WILL BE WRITTEN ON {} (CYLINDERS 0-{}): ITS CONTENT THERE IS LOST.{warn} CONTINUE?",
                list.join(", "),
                kickstart::KICK_CYLINDERS - 1
            ),
        });
        let tracks = hgf_core::gw::TrackOpts {
            custom: format!("c=0-{}:h=0-1", kickstart::KICK_CYLINDERS - 1),
            ..Default::default()
        };
        let geo = TrackSet::dense(kickstart::KICK_CYLINDERS, 2);
        let img = LastImage {
            path: temp,
            format: "amiga.amigados".into(),
            tracks,
            geo,
            name: short_name(&rom_path, 16),
        };
        let job = WriteJob {
            img,
            verb: "WRITING KICKSTART",
            no_verify: false,
            pre_erase: false,
            retries: None,
            hfreq_erase: false,
        };
        let done = format!("KICKSTART DISK READY: {what}.");
        v.extend(self.write_tasks(&job, &targets, None));
        v.push(Task::Done { message: done.clone() });
        // REPEAT writes more Kickstart disks without asking for the ROM again.
        self.last_op = Some((OpKey::Tool(ToolMode::KickDisk), Redo::Write { job, done }));
        self.launch(v);
    }
}
