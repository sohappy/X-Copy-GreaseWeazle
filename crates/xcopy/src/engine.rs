// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Operation engine: a queue of steps (gw commands, "insert disk" requesters, checks) run one
//! after the other, like X-COPY's copy loop. The gw process itself is run, parsed and mapped by
//! `hgf-core` (runner, parser, disk map).

use crate::sound::Sound;
use hgf_core::diag::hints;
use hgf_core::disk::diskmap::MapMode;
use hgf_core::fs::amigados::{self, Boot};
use hgf_core::gw::parse::{Output, TrackOutcome};
use hgf_core::gw::spawn::{JobControl, ProcessSpawner, Spawner};
use hgf_core::gw::{Action, ActionKind, Drive, GwConfig};
use hgf_core::{DiskMap, JobEvent, OutputParser, TrackSet};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Cmd,
    Info,
    Good,
    Warn,
    Error,
}

/// Builds the next steps when reached (see [`Task::Expand`]).
pub type ExpandFn = Box<dyn FnOnce(&mut Engine) -> Result<Vec<Task>, String>>;

/// One step of an operation.
pub enum Task {
    /// Run gw. `idx` is the DFx number of the drive (for its activity light).
    Gw {
        action: Box<Action>,
        drive: Drive,
        idx: usize,
        label: String,
        geometry: TrackSet,
    },
    /// Yes/No requester; "No" cancels the whole operation.
    Confirm {
        title: String,
        text: String,
    },
    /// "Insert disk" requester; Cancel stops the operation.
    Prompt {
        title: String,
        text: String,
    },
    /// After a read: ask before copying a source with unreadable tracks.
    CheckBad,
    /// Show the AmigaDOS directory of an image.
    Dir {
        path: PathBuf,
    },
    /// Show the device fields collected by `gw info`.
    Info,
    /// Report the result of the disk check.
    CheckDone,
    /// Report the measured drive speed.
    RpmDone,
    Done {
        message: String,
    },
    /// Steps computed when reached (e.g. BAMCOPY: which tracks to read once the bitmap is known).
    Expand(ExpandFn),
    /// Read the volume name of a probe image (cylinder 40) for the DF>ADF file name.
    NameProbe {
        path: PathBuf,
        hd: bool,
    },
    /// Classify the boot block of a freshly read image and offer to replace an unknown one.
    BootCheck {
        path: PathBuf,
        hd: bool,
    },
}

/// A requester the UI must show.
#[derive(Clone, Debug)]
pub struct Ask {
    pub title: String,
    pub lines: Vec<(String, Level)>,
    /// `true`: YES / NO, `false`: OK / CANCEL. Ignored for notices.
    pub yes_no: bool,
    /// Information only: OK closes it and the queue goes on.
    pub notice: bool,
    /// Custom buttons; the second one does not cancel the operation.
    pub choice: Option<(&'static str, &'static str)>,
}

struct Run {
    job: Box<dyn JobControl>,
    kind: ActionKind,
    label: String,
    failure: Option<String>,
}

pub struct Engine {
    pub cfg: GwConfig,
    /// Starts gw: the real program, or a scripted replay in tests.
    spawner: Box<dyn Spawner>,
    queue: VecDeque<Task>,
    run: Option<Run>,
    parser: OutputParser,
    pub map: DiskMap,
    pub log: Vec<(String, Level)>,
    pub status: String,
    pub level: Level,
    pub ask: Option<Ask>,
    /// Drive currently in use and whether it is writing.
    pub active: Option<(usize, bool)>,
    started: Option<Instant>,
    elapsed: Duration,
    info: Vec<(String, String)>,
    rpm: Vec<f64>,
    rpm_mean: Option<f64>,
    pub device_ok: Option<bool>,
    pub model: String,
    /// Number of operations that reached their final `Done` step.
    pub done_count: u64,
    /// Jingle to play (taken by the UI).
    pub sound: Option<Sound>,
    /// Result of a [`Task::NameProbe`]: `Some(name)` once probed (`None` inside = no name).
    pub probe_name: Option<Option<String>>,
    pending_yes: Vec<Task>,
}

impl Engine {
    pub fn new(cfg: GwConfig) -> Self {
        Self::with_spawner(cfg, Box::new(ProcessSpawner))
    }

    pub fn with_spawner(cfg: GwConfig, spawner: Box<dyn Spawner>) -> Self {
        Self {
            cfg,
            spawner,
            queue: VecDeque::new(),
            run: None,
            parser: OutputParser::new(),
            map: DiskMap::new(MapMode::Read, TrackSet::dense(80, 2)),
            log: Vec::new(),
            status: "READY.".into(),
            level: Level::Good,
            ask: None,
            active: None,
            started: None,
            elapsed: Duration::ZERO,
            info: Vec::new(),
            rpm: Vec::new(),
            rpm_mean: None,
            device_ok: None,
            model: String::new(),
            done_count: 0,
            sound: None,
            probe_name: None,
            pending_yes: Vec::new(),
        }
    }

    pub fn busy(&self) -> bool {
        self.run.is_some() || !self.queue.is_empty() || (self.ask.is_some() && self.started.is_some())
    }

    pub fn running_gw(&self) -> bool {
        self.run.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        match self.started {
            Some(t) => t.elapsed(),
            None => self.elapsed,
        }
    }

    pub fn push_log(&mut self, s: impl Into<String>, l: Level) {
        self.log.push((s.into(), l));
        if self.log.len() > 3000 {
            self.log.drain(..500);
        }
    }

    pub fn set_status(&mut self, s: impl Into<String>, l: Level) {
        self.status = s.into();
        self.level = l;
    }

    /// Start a new operation.
    pub fn start(&mut self, tasks: Vec<Task>, cfg: GwConfig) {
        if self.busy() {
            return;
        }
        self.cfg = cfg;
        self.queue = tasks.into();
        self.started = Some(Instant::now());
        self.elapsed = Duration::ZERO;
        self.map = DiskMap::new(MapMode::Read, TrackSet::dense(80, 2));
        self.map.geometry = TrackSet { cyls: vec![], heads: vec![0, 1] };
    }

    /// STOP: interrupt gw (cleanly) and drop the rest of the operation.
    pub fn stop(&mut self) {
        if let Some(r) = &self.run {
            r.job.cancel();
            self.push_log("Stopping… (the drive is released cleanly)", Level::Warn);
        }
        if !self.queue.is_empty() || self.ask.is_some() {
            self.queue.clear();
            if self.run.is_none() {
                self.ask = None;
                self.finish("OPERATION STOPPED.", Level::Warn);
            }
        }
    }

    /// Answer to the current requester.
    pub fn answer(&mut self, yes: bool) {
        let Some(a) = self.ask.take() else { return };
        if a.choice.is_some() {
            let tasks = std::mem::take(&mut self.pending_yes);
            if yes {
                for t in tasks.into_iter().rev() {
                    self.queue.push_front(t);
                }
            }
            return;
        }
        if !yes && !a.notice {
            self.queue.clear();
            self.finish("OPERATION CANCELLED.", Level::Warn);
        }
    }

    fn finish(&mut self, msg: &str, l: Level) {
        if let Some(t) = self.started.take() {
            self.elapsed = t.elapsed();
        }
        self.active = None;
        self.pending_yes.clear();
        self.sound = Some(match l {
            Level::Error => Sound::Error,
            Level::Warn if msg.contains("STOPPED") || msg.contains("CANCELLED") => Sound::Abort,
            _ => Sound::Done,
        });
        self.set_status(msg, l);
    }

    fn fail(&mut self, kind: Option<ActionKind>, message: String) {
        self.queue.clear();
        let mut lines = vec![(message.clone(), Level::Error)];
        if let Some(h) = kind.and_then(|k| hints::for_failure(k, &message)) {
            lines.push((String::new(), Level::Info));
            lines.push((h.title.clone(), Level::Warn));
            for s in &h.steps {
                lines.push((format!("- {s}"), Level::Info));
            }
        }
        self.finish(&format!("ERROR: {}", message.to_uppercase()), Level::Error);
        self.ask =
            Some(Ask { title: "OPERATION FAILED".into(), lines, yes_no: false, notice: true, choice: None });
    }

    /// Called every frame: drain gw output, advance the queue. Returns true while animating.
    pub fn tick(&mut self) -> bool {
        let mut events = Vec::new();
        if let Some(r) = &self.run {
            while let Some(e) = r.job.try_next() {
                events.push(e);
                if events.len() > 2000 {
                    break;
                }
            }
        }
        for e in events {
            match e {
                JobEvent::Line(l) => self.on_line(l),
                JobEvent::Partial(p) => {
                    if let Some(r) = &self.run {
                        let msg = format!("{} {}", r.label, p.trim());
                        self.set_status(msg, Level::Info);
                    }
                }
                JobEvent::Finished { code, cancelled } => self.on_finished(code, cancelled),
            }
        }
        if let (Some(r), Some((c, h))) = (&self.run, self.map.active) {
            self.status = format!("{}  TRACK {c:02} SIDE {h}", r.label);
            self.level = Level::Info;
        }
        while self.run.is_none() && self.ask.is_none() {
            let Some(t) = self.queue.pop_front() else { break };
            self.exec(t);
        }
        self.busy()
    }

    fn exec(&mut self, t: Task) {
        match t {
            Task::Gw { action, drive, idx, label, geometry } => {
                let kind = action.kind();
                let args = action.args(&self.cfg, drive);
                self.push_log(format!("$ {}", action.command_line(&self.cfg, drive)), Level::Cmd);
                if kind.uses_disk_map() {
                    let mode = match kind {
                        ActionKind::Write => MapMode::Write,
                        ActionKind::Erase => MapMode::Erase,
                        _ => MapMode::Read,
                    };
                    self.map = DiskMap::new(mode, geometry);
                }
                match kind {
                    ActionKind::Info => self.info.clear(),
                    ActionKind::Rpm => {
                        self.rpm.clear();
                        self.rpm_mean = None;
                    }
                    _ => {}
                }
                self.parser = OutputParser::new();
                match self.spawner.spawn(&self.cfg.program, &args) {
                    Ok(job) => {
                        let writes =
                            matches!(kind, ActionKind::Write | ActionKind::Erase | ActionKind::Clean);
                        self.active = (kind != ActionKind::Info).then_some((idx, writes));
                        self.set_status(label.clone(), Level::Info);
                        self.run = Some(Run { job, kind, label, failure: None });
                    }
                    Err(e) => {
                        if e.contains("not found") {
                            self.device_ok = None;
                        }
                        self.push_log(e.clone(), Level::Error);
                        self.fail(Some(kind), e);
                    }
                }
            }
            Task::Confirm { title, text } => {
                self.ask = Some(Ask {
                    title,
                    lines: vec![(text, Level::Warn)],
                    yes_no: true,
                    notice: false,
                    choice: None,
                });
                self.set_status("WAITING FOR CONFIRMATION...", Level::Warn);
            }
            Task::Prompt { title, text } => {
                self.active = None;
                self.ask = Some(Ask {
                    title,
                    lines: vec![(text, Level::Info)],
                    yes_no: false,
                    notice: false,
                    choice: None,
                });
                self.set_status("INSERT DISK AND CLICK OK.", Level::Warn);
            }
            Task::CheckBad => {
                let bad = self.map.bad_tracks();
                if !bad.is_empty() {
                    let list: Vec<String> = bad.iter().take(12).map(|(c, h)| format!("{c}.{h}")).collect();
                    self.ask = Some(Ask {
                        title: "READ ERRORS".into(),
                        lines: vec![
                            (format!("{} track(s) of the source could not be read:", bad.len()), Level::Warn),
                            (list.join(" ") + if bad.len() > 12 { " ..." } else { "" }, Level::Error),
                            (String::new(), Level::Info),
                            ("Write the copy anyway (bad sectors are left blank)?".into(), Level::Info),
                        ],
                        yes_no: true,
                        notice: false,
                        choice: None,
                    });
                    self.set_status(format!("{} BAD TRACK(S) ON THE SOURCE.", bad.len()), Level::Warn);
                }
            }
            Task::Dir { path } => {
                let lines = dir_lines(&path);
                self.ask =
                    Some(Ask { title: "DIRECTORY".into(), lines, yes_no: false, notice: true, choice: None });
            }
            Task::Info => {
                let mut lines: Vec<(String, Level)> =
                    self.info.iter().map(|(k, v)| (format!("{k:<10} {v}"), Level::Info)).collect();
                if lines.is_empty() {
                    lines.push(("No Greaseweazle found.".into(), Level::Error));
                }
                self.ask =
                    Some(Ask { title: "DISK INFO".into(), lines, yes_no: false, notice: true, choice: None });
                self.finish("OPERATION DONE.", Level::Good);
            }
            Task::CheckDone => {
                let c = self.map.counts();
                let (msg, l) = if c.bad == 0 && c.empty == 0 {
                    (format!("CHECKDISK: {} TRACKS OK, NO ERRORS.", c.ok + c.recovered), Level::Good)
                } else {
                    (
                        format!(
                            "CHECKDISK: {} OK, {} BAD, {} EMPTY TRACKS.",
                            c.ok + c.recovered,
                            c.bad,
                            c.empty
                        ),
                        Level::Warn,
                    )
                };
                self.finish(&msg, l);
            }
            Task::RpmDone => {
                let mean = self.rpm_mean.or_else(|| {
                    (!self.rpm.is_empty()).then(|| self.rpm.iter().sum::<f64>() / self.rpm.len() as f64)
                });
                match mean {
                    Some(m) => {
                        let dev = (m - 300.0).abs() / 3.0;
                        let l = if dev < 1.0 { Level::Good } else { Level::Warn };
                        self.finish(&format!("DRIVE SPEED: {m:.2} RPM ({:+.2}%)", (m - 300.0) / 3.0), l);
                    }
                    None => self.finish("SPEED CHECK: NO INDEX PULSE.", Level::Error),
                }
            }
            Task::Done { message } => {
                self.done_count += 1;
                self.finish(&message, Level::Good);
            }
            Task::Expand(f) => match f(self) {
                Ok(tasks) => {
                    for t in tasks.into_iter().rev() {
                        self.queue.push_front(t);
                    }
                }
                Err(e) => {
                    self.push_log(e.clone(), Level::Error);
                    self.fail(None, e);
                }
            },
            Task::NameProbe { path, hd } => {
                let data = std::fs::read(&path).unwrap_or_default();
                let name = amigados::volume_name(&data, hd);
                match &name {
                    Some(n) => self.push_log(format!("Volume name: {n}"), Level::Good),
                    None => self.push_log("No volume name found (NDOS or damaged root block).", Level::Warn),
                }
                let msg = match &name {
                    Some(n) => format!("VOLUME: {}. CHOOSE THE FILE NAME...", n.to_uppercase()),
                    None => "NO VOLUME NAME. CHOOSE THE FILE NAME...".to_string(),
                };
                self.probe_name = Some(name);
                self.finish(&msg, Level::Good);
                self.sound = None;
            }
            Task::BootCheck { path, hd } => {
                let Ok(data) = std::fs::read(&path) else { return };
                let boot = amigados::classify_boot(&data);
                self.push_log(format!("Boot block: {}", boot.describe()), Level::Info);
                if boot.is_unusual() {
                    let fix = path.clone();
                    let patch: Task = Task::Expand(Box::new(move |e: &mut Engine| {
                        let mut d = std::fs::read(&fix).map_err(|e| e.to_string())?;
                        amigados::blank_boot(&mut d, hd);
                        std::fs::write(&fix, &d).map_err(|e| e.to_string())?;
                        e.push_log("Boot block replaced by a blank one in the image.", Level::Good);
                        Ok(Vec::new())
                    }));
                    let lvl = if matches!(boot, Boot::Suspect(_)) { Level::Error } else { Level::Warn };
                    self.pending_yes = vec![patch];
                    self.ask = Some(Ask {
                        title: "BOOT BLOCK".into(),
                        lines: vec![
                            (format!("Boot block: {}", boot.describe()), lvl),
                            (String::new(), Level::Info),
                            ("Unknown boot code: a game or demo loader, or a boot block virus.".into(), Level::Info),
                            ("Games and demos often NEED their boot block: keep it for them.".into(), Level::Warn),
                            ("REPLACE writes a blank boot block (files stay readable, the disk no longer boots).".into(), Level::Info),
                        ],
                        yes_no: false,
                        notice: false,
                        choice: Some(("REPLACE", "KEEP")),
                    });
                    self.set_status(format!("BOOT BLOCK: {}", boot.describe().to_uppercase()), lvl);
                }
            }
        }
    }

    fn on_line(&mut self, line: String) {
        let Some(kind) = self.run.as_ref().map(|r| r.kind) else { return };
        let fatal_message = self.parser.is_fatal_message();
        let out = self.parser.feed(&line);
        let level = if out.is_error() || fatal_message {
            Level::Error
        } else if out.is_warning() {
            Level::Warn
        } else if matches!(out, Output::AllVerified | Output::Found { .. }) {
            Level::Good
        } else {
            Level::Info
        };
        let failure = match &out {
            _ if fatal_message => Some(line.trim().to_string()),
            Output::CommandFailed(m) => Some(m.clone()),
            _ => None,
        };
        if let (Some(f), Some(r)) = (failure, self.run.as_mut()) {
            r.failure.get_or_insert(f);
        }
        match &out {
            Output::KeyValue { key, value } if kind == ActionKind::Info => {
                if key == "Model" {
                    self.model = value.clone();
                    self.device_ok = Some(true);
                }
                self.info.push((key.clone(), value.clone()));
            }
            Output::DeviceNotFound => self.device_ok = Some(false),
            Output::Rpm { rpm, .. } => self.rpm.push(*rpm),
            Output::RpmStat { label, rpm } if label == "Ar.Mean" => self.rpm_mean = Some(*rpm),
            Output::Track { outcome: TrackOutcome::Decoded { .. }, .. } => {}
            _ => {}
        }
        if kind.uses_disk_map() {
            self.map.apply(&out);
        }
        self.push_log(line, level);
    }

    fn on_finished(&mut self, code: Option<i32>, cancelled: bool) {
        let Some(r) = self.run.take() else { return };
        let success = code == Some(0) && r.failure.is_none() && !cancelled;
        if r.kind.uses_disk_map() {
            self.map.finish(success);
        }
        if success {
            if r.kind != ActionKind::Info {
                self.device_ok = Some(true);
            }
            return;
        }
        if cancelled {
            self.queue.clear();
            self.ask = None;
            self.finish("OPERATION STOPPED.", Level::Warn);
            return;
        }
        let msg = r.failure.unwrap_or_else(|| match code {
            Some(c) => format!("gw exited with status {c}."),
            None => "gw was terminated.".into(),
        });
        if r.kind == ActionKind::Info {
            self.device_ok = Some(false);
        }
        self.fail(Some(r.kind), msg);
    }
}

/// Directory listing of an ADF, for the DIRECTORY requester.
pub fn dir_lines(path: &std::path::Path) -> Vec<(String, Level)> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return vec![(format!("Cannot read the image: {e}"), Level::Error)],
    };
    if amigados::kickstart::is_kick_disk(&data) {
        return match amigados::kickstart::rom_from_kick_disk(&data) {
            Ok(k) => vec![
                ("AMIGA 1000 KICKSTART DISK".into(), Level::Good),
                (k.describe(), Level::Info),
                (
                    format!("ROM checksum: {}", if k.checksum_ok { "OK" } else { "WRONG (damaged disk?)" }),
                    if k.checksum_ok { Level::Info } else { Level::Error },
                ),
            ],
            Err(e) => vec![("AMIGA 1000 KICKSTART DISK".into(), Level::Warn), (e, Level::Error)],
        };
    }
    match amigados::inspect_adf(&data) {
        Err(e) => vec![(e, Level::Error), ("Not an AmigaDOS disk (NDOS)?".into(), Level::Warn)],
        Ok(info) => {
            let mut v = vec![
                (format!("VOLUME: {}", info.volume), Level::Good),
                (
                    format!(
                        "{}  {}{}",
                        info.filesystem,
                        if info.bootable { "BOOTABLE" } else { "NOT BOOTABLE" },
                        if info.root_checksum_ok { "" } else { "  ROOT CHECKSUM BAD!" }
                    ),
                    if info.root_checksum_ok { Level::Info } else { Level::Error },
                ),
                {
                    let b = amigados::classify_boot(&data);
                    let l = match b {
                        Boot::Suspect(_) => Level::Error,
                        Boot::Custom => Level::Warn,
                        _ => Level::Info,
                    };
                    (format!("BOOT BLOCK: {}", b.describe().to_uppercase()), l)
                },
                (String::new(), Level::Info),
            ];
            let mut entries = info.entries.clone();
            entries.sort_by(|a, b| {
                b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            for e in &entries {
                let size = if e.is_dir { "(DIR)".to_string() } else { e.size.to_string() };
                let name: String = e.name.chars().take(30).collect();
                v.push((format!("{name:<31}{size:>9}"), if e.is_dir { Level::Warn } else { Level::Info }));
            }
            if entries.is_empty() {
                v.push(("(empty disk)".into(), Level::Info));
            }
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hgf_core::gw::spawn::{Script, ScriptedSpawner};
    use hgf_core::gw::{ReadOpts, WriteOpts};

    const READ_OK: &str = "Reading c=0-1:h=0-1 revs=3
Format amiga.amigados
T0.0: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms)
T0.1: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms)
T1.0: AmigaDOS (9/11 sectors) from Raw Flux (100000 flux in 600.00ms)
T1.0: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms) (Retry #0.1)
T1.1: AmigaDOS (11/11 sectors) from Raw Flux (100000 flux in 600.00ms)";

    const WRITE_OK: &str = "Format amiga.amigados
Writing c=0-1:h=0-1
T0.0: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)
T0.1: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)
T1.0: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)
T1.1: Writing Track (Flux: 200.0ms period, 200.0 ms total, Write all)
All tracks verified";

    const WRITE_FAIL: &str = "Format amiga.amigados
Writing c=0-1:h=0-1
T0.0: Writing Track (Flux: 200.0ms period, 219.9 ms total, Terminate at index)
T0.1: Writing Track (Verify Failure: Retry #3)
** FATAL ERROR:
Failed to verify Track 0.1";

    fn geo() -> TrackSet {
        TrackSet::dense(2, 2)
    }

    fn read_task() -> Task {
        let action = Action::Read(ReadOpts {
            output: "copy.adf".into(),
            format: "amiga.amigados".into(),
            ..Default::default()
        });
        Task::Gw {
            action: Box::new(action),
            drive: Drive::A,
            idx: 0,
            label: "READING DF0".into(),
            geometry: geo(),
        }
    }

    fn write_task() -> Task {
        let action = Action::Write(WriteOpts {
            input: "copy.adf".into(),
            format: "amiga.amigados".into(),
            ..Default::default()
        });
        Task::Gw {
            action: Box::new(action),
            drive: Drive::B,
            idx: 1,
            label: "WRITING DF1".into(),
            geometry: geo(),
        }
    }

    fn copy_tasks() -> Vec<Task> {
        vec![
            Task::Confirm { title: "WARNING!".into(), text: "ALL DATA ON DF1 WILL BE DESTROYED.".into() },
            read_task(),
            Task::CheckBad,
            write_task(),
            Task::Done { message: "OPERATION DONE.".into() },
        ]
    }

    fn engine(scripts: Vec<Script>) -> (Engine, ScriptedSpawner) {
        let spawner = ScriptedSpawner::new(scripts);
        let probe = spawner.clone();
        (Engine::with_spawner(GwConfig::default(), Box::new(spawner)), probe)
    }

    /// Tick until the engine is idle or waits for the user.
    fn run(e: &mut Engine) {
        for _ in 0..10_000 {
            e.tick();
            if e.ask.is_some() || !e.busy() {
                return;
            }
        }
        panic!("engine did not settle");
    }

    #[test]
    fn disk_to_disk_copy_reads_then_writes() {
        let (mut e, gw) = engine(vec![Script::new(READ_OK, 0), Script::new(WRITE_OK, 0)]);
        e.start(copy_tasks(), GwConfig::default());
        run(&mut e);
        assert_eq!(e.ask.as_ref().map(|a| a.title.as_str()), Some("WARNING!"));
        assert!(gw.calls().is_empty(), "nothing runs before the confirmation");
        e.answer(true);
        run(&mut e);
        assert!(e.ask.is_none());
        assert!(!e.busy());
        assert_eq!(e.status, "OPERATION DONE.");
        assert_eq!(e.done_count, 1);
        assert_eq!(e.sound, Some(Sound::Done));
        let calls = gw.calls();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].starts_with("gw read --drive A --format amiga.amigados"), "{}", calls[0]);
        assert!(calls[1].starts_with("gw write --drive B --format amiga.amigados"), "{}", calls[1]);
        // the map shows the last operation: four tracks written and verified
        assert_eq!(e.map.counts().ok, 4);
    }

    #[test]
    fn verify_failure_stops_with_an_explanation() {
        let (mut e, _) = engine(vec![Script::new(READ_OK, 0), Script::new(WRITE_FAIL, 1)]);
        e.start(copy_tasks(), GwConfig::default());
        run(&mut e);
        e.answer(true);
        run(&mut e);
        let ask = e.ask.clone().expect("failure requester");
        assert_eq!(ask.title, "OPERATION FAILED");
        assert!(ask.notice);
        assert!(ask.lines[0].0.contains("Failed to verify Track 0.1"));
        assert!(ask.lines.len() > 2, "a plain-language hint follows the error");
        assert!(e.status.starts_with("ERROR:"));
        assert_eq!(e.level, Level::Error);
        assert_eq!(e.done_count, 0);
        assert_eq!(e.sound, Some(Sound::Error));
        e.answer(true);
        assert!(!e.busy());
    }

    #[test]
    fn no_at_the_confirmation_runs_nothing() {
        let (mut e, gw) = engine(vec![Script::new(READ_OK, 0)]);
        e.start(copy_tasks(), GwConfig::default());
        run(&mut e);
        e.answer(false);
        run(&mut e);
        assert!(gw.calls().is_empty());
        assert_eq!(e.status, "OPERATION CANCELLED.");
        assert!(!e.busy());
    }

    #[test]
    fn stop_interrupts_the_running_job() {
        let (mut e, gw) = engine(vec![Script::new(READ_OK, 0), Script::new(WRITE_OK, 0)]);
        e.start(vec![read_task(), write_task()], GwConfig::default());
        e.tick(); // the read starts and its first lines are consumed
        e.stop();
        run(&mut e);
        assert_eq!(e.status, "OPERATION STOPPED.");
        assert_eq!(gw.calls().len(), 1, "the write never starts");
        assert_eq!(e.sound, Some(Sound::Abort));
    }

    #[test]
    fn expand_inserts_steps_in_place() {
        let (mut e, gw) = engine(vec![Script::new(READ_OK, 0)]);
        let tasks = vec![
            Task::Expand(Box::new(|_| Ok(vec![read_task()]))),
            Task::Done { message: "OPERATION DONE.".into() },
        ];
        e.start(tasks, GwConfig::default());
        run(&mut e);
        assert_eq!(gw.calls().len(), 1);
        assert_eq!(e.status, "OPERATION DONE.");
    }

    #[test]
    fn directory_recognises_a_kickstart_disk() {
        let mut rom = vec![0u8; 256 * 1024];
        rom[..4].copy_from_slice(&[0x11, 0x11, 0x4E, 0xF9]);
        rom[12..16].copy_from_slice(&[0, 33, 0, 180]);
        let k = amigados::kickstart::load_rom(&rom, None).unwrap();
        let p = std::env::temp_dir().join("xcopy-test-kick.adf");
        std::fs::write(&p, amigados::kickstart::kick_disk_image(&k)).unwrap();
        let lines = dir_lines(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(lines[0].0, "AMIGA 1000 KICKSTART DISK");
        assert_eq!(lines[1].0, "Kickstart 1.2 (33.180)");
        assert!(lines[2].0.contains("WRONG"), "this synthetic ROM has no valid checksum");
    }

    #[test]
    fn directory_of_a_blank_disk() {
        let opts = hgf_core::fs::amigados::FormatOpts { name: "Games".into(), ..Default::default() };
        let img = hgf_core::fs::amigados::blank_image(&opts).unwrap();
        let p = std::env::temp_dir().join("xcopy-test-dir.adf");
        std::fs::write(&p, img).unwrap();
        let lines = dir_lines(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(lines[0].0, "VOLUME: Games");
        assert!(lines.iter().any(|l| l.0 == "(empty disk)"));
    }
}
