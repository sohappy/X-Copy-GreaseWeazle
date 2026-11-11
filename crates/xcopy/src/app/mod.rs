// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! The X-COPY application: state, settings, the click dispatcher and the eframe loop.
//! Operations, dialogs and drawing live in the sub-modules.

mod dialogs;
mod ops;
mod screen;

use crate::engine::{Engine, Level};
use crate::settings::*;
use crate::ui::font::Font;
use crate::ui::gfx::{self, pal, Canvas, Gfx};
use eframe::egui::{Color32, Key, TextureHandle, Ui};
use hgf_core::disk::naming::safe_file_stem;
use hgf_core::gw::{locate, runner};
use hgf_core::TrackSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

const STORAGE_KEY: &str = "xcopy-settings";

/// Everything a click on the screen can do.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Start,
    Repeat,
    Stop,
    DiskInfo,
    Directory,
    CheckDisk,
    Default,
    Section(Section),
    Mode(i32),
    StartAdj(i32),
    EndAdj(i32),
    Side,
    Dens,
    Flow,
    ToolDevice,
    Fs,
    EditName,
    Source(usize),
    Target(usize),
    Setup,
    Log,
    ModalButton(usize),
    ModalField(usize),
    ModalScroll(i32),
    Bus,
    SoundToggle,
    ConfirmToggle,
    RereadToggle,
    PrecompToggle,
    KeepFluxToggle,
    PortNext,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    /// Requester asked by the engine (confirm, insert disk, error, directory…).
    Engine,
    Setup,
    Name,
    Log,
}

struct Field {
    label: &'static str,
    value: String,
    max: usize,
}

struct Modal {
    kind: ModalKind,
    title: String,
    lines: Vec<(String, Level)>,
    buttons: Vec<&'static str>,
    fields: Vec<Field>,
    focus: usize,
    scroll: usize,
    error: Option<String>,
    bus: Bus,
    sound: bool,
    confirm: bool,
    reread: bool,
    precomp: bool,
    keep_flux: bool,
}

impl Modal {
    fn new(kind: ModalKind, title: &str, lines: Vec<(String, Level)>, buttons: Vec<&'static str>) -> Self {
        Self {
            kind,
            title: title.into(),
            lines,
            buttons,
            fields: Vec::new(),
            focus: 0,
            scroll: 0,
            error: None,
            bus: Bus::Ibm,
            sound: true,
            confirm: true,
            reread: true,
            precomp: false,
            keep_flux: false,
        }
    }
}

/// What to do with the file chosen in the (asynchronous) file dialog.
enum Pick {
    /// DF>ADF: read the source disk into this file.
    ReadTo,
    /// ADF>DF: write this image to the target disks.
    WriteFrom,
    /// TOOLS/FORMAT with DEVICE=ADF: save this blank image.
    SaveBlank(Vec<u8>),
    /// DIRECTORY of an image file.
    Dir,
    /// KICKDISK: the Kickstart ROM image to put on an A1000 Kickstart disk.
    KickRom,
}

/// Result of the background search for gw and the Greaseweazle.
#[derive(Default)]
struct Detected {
    program: Option<String>,
    model: Option<String>,
    port: Option<String>,
}

/// Image kept after a copy, so REPEAT writes more copies without reading the source again.
#[derive(Clone)]
struct LastImage {
    path: PathBuf,
    format: String,
    tracks: hgf_core::gw::TrackOpts,
    geo: TrackSet,
    /// File name shown while writing (`GAME.ADF`); empty for internal images.
    name: String,
}

/// How an image is written on each target disk.
#[derive(Clone)]
struct WriteJob {
    img: LastImage,
    /// Start of the status label: `WRITING`, `FORMATTING`…
    verb: &'static str,
    no_verify: bool,
    pre_erase: bool,
    retries: Option<u32>,
    /// TRACK0: high-frequency erase of the tracks before writing them (old stubborn signal).
    hfreq_erase: bool,
}

/// Which command an operation belongs to: REPEAT replays the last operation of the command
/// selected on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpKey {
    Copy(Flow),
    Tool(ToolMode),
}

/// What REPEAT does again, without asking for any file.
#[derive(Clone)]
enum Redo {
    /// Write the same image on new disks (ADF>DF, DF>DF once read, FORMAT, TRACK0, KICKDISK).
    Write { job: WriteJob, done: String },
    /// Erase the same tracks.
    Erase { tracks: hgf_core::gw::TrackOpts, geo: TrackSet },
    /// DF>ADF: read the next disk into the same folder, named after its volume.
    ReadTo { dir: Option<PathBuf>, ext: &'static str },
}

pub struct App {
    s: Settings,
    eng: Engine,
    font: Font,
    dots: TextureHandle,
    modal: Option<Modal>,
    /// Last operation, replayed by REPEAT.
    last_op: Option<(OpKey, Redo)>,
    /// Operation recorded once the running task list completes (DF>DF: the image exists only
    /// after a successful read).
    pending_op: Option<(u64, (OpKey, Redo))>,
    /// DF>ADF repeated: save in this folder without asking.
    auto_save: Option<Option<PathBuf>>,
    /// Image being written or read, shown on the drive panel.
    image_name: Option<(Flow, String)>,
    /// File dialog open in the background (never blocks the window).
    picking: Option<(Pick, Receiver<Option<PathBuf>>)>,
    temps: Vec<PathBuf>,
    detect_rx: Option<Receiver<Detected>>,
    /// Serial ports offered in SETUP (Greaseweazles first).
    ports: Vec<hgf_core::gw::ports::Port>,
    gw_found: Option<bool>,
    help: Option<String>,
    status_seen: String,
    status_since: Instant,
    t0: Instant,
}

/// `dir/stem.ext`, or `dir/stem_2.ext`, `stem_3`… when the file already exists.
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem}_{n}.{ext}"));
        n += 1;
    }
    p
}

/// Upper-case file name of `path`, shortened to `max` characters for the screen.
fn short_name(path: &Path, max: usize) -> String {
    let n = path.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
    if n.chars().count() <= max {
        n
    } else {
        let keep: String = n.chars().take(max.saturating_sub(2)).collect();
        format!("{keep}..")
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xcopy-modern-{}-{name}", std::process::id()))
}

/// Remove temporary images left by sessions that were killed (older than 12 hours).
fn clean_stale_temps() {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("xcopy-modern-") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d.as_secs() > 12 * 3600);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Cmd => pal::CYAN,
        Level::Info => pal::CYAN,
        Level::Good => pal::GREEN,
        Level::Warn => pal::YELLOW,
        Level::Error => pal::RED,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut s: Settings =
            cc.storage.and_then(|st| eframe::get_value(st, STORAGE_KEY)).unwrap_or_default();
        if let Some(p) = std::env::var_os("XCOPY_GW").or_else(|| std::env::var_os("HGF_GW")) {
            s.cfg.program = p.to_string_lossy().into_owned();
        }
        if s.end < s.start || s.end > MAX_CYL {
            s.start = 0;
            s.end = 79;
        }
        let ctx = &cc.egui_ctx;
        let mut app = Self {
            eng: Engine::new(s.cfg.clone()),
            s,
            font: Font::new(ctx),
            dots: gfx::dots_texture(ctx),
            modal: None,
            last_op: None,
            pending_op: None,
            auto_save: None,
            image_name: None,
            picking: None,
            temps: Vec::new(),
            detect_rx: None,
            ports: Vec::new(),
            gw_found: None,
            help: None,
            status_seen: String::new(),
            status_since: Instant::now(),
            t0: Instant::now(),
        };
        clean_stale_temps();
        app.detect();
        app.eng.push_log("X-COPY Modern ready. Greaseweazle host tools: searching...", Level::Info);
        app
    }

    /// Find gw, then ask it which Greaseweazle is connected and on which port (background).
    fn detect(&mut self) {
        let (tx, rx) = mpsc::channel();
        let pref = self.s.cfg.program.clone();
        let device = self.s.cfg.device.clone();
        std::thread::spawn(move || {
            let program = locate::detect(&pref);
            let mut d = Detected { program: program.clone(), ..Default::default() };
            if let Some(p) = &program {
                let mut args = vec!["info"];
                if !device.trim().is_empty() {
                    args.extend(["--device", device.trim()]);
                }
                if let Ok(out) = runner::capture(p, &args, std::time::Duration::from_secs(8)) {
                    for line in out.lines() {
                        let line = line.trim();
                        if let Some(v) = line.strip_prefix("Port:") {
                            d.port = Some(v.trim().to_string());
                        } else if let Some(v) = line.strip_prefix("Model:") {
                            d.model = Some(v.trim().to_string());
                        }
                    }
                }
            }
            let _ = tx.send(d);
        });
        self.detect_rx = Some(rx);
        self.gw_found = None;
    }

    fn poll_detect(&mut self) {
        let Some(rx) = &self.detect_rx else { return };
        let Ok(found) = rx.try_recv() else { return };
        self.detect_rx = None;
        match found.program {
            Some(p) => {
                self.eng.push_log(format!("Greaseweazle host tools: {p}"), Level::Good);
                self.s.cfg.program = p;
                self.gw_found = Some(true);
                let device = match (&found.model, &found.port) {
                    (Some(model), Some(port)) => {
                        self.eng.push_log(format!("{model} on {port}"), Level::Good);
                        self.eng.model = model.clone();
                        self.eng.port = port.clone();
                        self.eng.device_ok = Some(true);
                        format!("{} ON {port}", model.to_uppercase())
                    }
                    _ => {
                        self.eng.push_log("No Greaseweazle answered (unplugged?).", Level::Warn);
                        self.eng.device_ok = Some(false);
                        "NO GREASEWEAZLE CONNECTED".into()
                    }
                };
                if let Some(m) = &mut self.modal {
                    if m.kind == ModalKind::Setup {
                        m.fields[0].value = self.s.cfg.program.clone();
                        if let Some(port) = &found.port {
                            m.fields[1].value = port.clone();
                        }
                        m.error = Some(format!("GW FOUND. {device}"));
                    }
                }
            }
            None => {
                self.gw_found = Some(false);
                self.eng.push_log(
                    "gw not found: install the Greaseweazle host tools (pip install greaseweazle) or set the path in SETUP.",
                    Level::Error,
                );
                if !self.eng.busy() {
                    self.eng.set_status("GW NOT FOUND - CLICK SETUP (TOP RIGHT).", Level::Error);
                }
                if let Some(m) = &mut self.modal {
                    if m.kind == ModalKind::Setup {
                        m.error = Some("GW NOT FOUND ON THIS COMPUTER.".into());
                    }
                }
            }
        }
    }

    // ───────────────────────────── operations ─────────────────────────────

    fn apply(&mut self, act: Act) {
        let busy = self.eng.busy() || self.picking.is_some();
        let s = &mut self.s;
        match act {
            Act::Start => self.start(),
            Act::Repeat => self.repeat(),
            Act::Stop => self.eng.stop(),
            Act::DiskInfo => self.disk_info(),
            Act::Directory => self.directory(),
            Act::CheckDisk => self.check_disk(),
            Act::Setup => self.open_setup(),
            Act::Log => self.open_log(),
            Act::ModalButton(i) => self.modal_button(i),
            Act::ModalField(i) => {
                if let Some(m) = &mut self.modal {
                    m.focus = i;
                }
            }
            Act::ModalScroll(d) => {
                if let Some(m) = &mut self.modal {
                    m.scroll = if d < 0 {
                        m.scroll.saturating_sub((-d) as usize)
                    } else {
                        m.scroll.saturating_add(d as usize)
                    };
                }
            }
            Act::ConfirmToggle => {
                if let Some(m) = &mut self.modal {
                    m.confirm = !m.confirm;
                }
            }
            Act::RereadToggle => {
                if let Some(m) = &mut self.modal {
                    m.reread = !m.reread;
                }
            }
            Act::PrecompToggle => {
                if let Some(m) = &mut self.modal {
                    m.precomp = !m.precomp;
                }
            }
            Act::KeepFluxToggle => {
                if let Some(m) = &mut self.modal {
                    m.keep_flux = !m.keep_flux;
                }
            }
            Act::PortNext => {
                // AUTO, then every serial port found (Greaseweazles first)
                if let Some(m) = &mut self.modal {
                    let names: Vec<String> = std::iter::once(String::new())
                        .chain(self.ports.iter().map(|p| p.name.clone()))
                        .collect();
                    let cur = names.iter().position(|n| *n == m.fields[1].value.trim()).unwrap_or(0);
                    m.fields[1].value = names[(cur + 1) % names.len()].clone();
                }
            }
            Act::SoundToggle => {
                if let Some(m) = &mut self.modal {
                    m.sound = !m.sound;
                    if m.sound {
                        crate::sound::play(crate::sound::Sound::Done);
                    }
                }
            }
            Act::Bus => {
                if let Some(m) = &mut self.modal {
                    m.bus = if m.bus == Bus::Ibm { Bus::Shugart } else { Bus::Ibm };
                }
            }
            _ if busy => {}
            Act::Default => {
                s.defaults();
                self.eng.set_status("DEFAULT SETTINGS RESTORED.", Level::Good);
            }
            Act::Section(sec) => {
                if s.section == sec {
                    self.apply(Act::Mode(1));
                } else {
                    s.section = sec;
                }
            }
            Act::Mode(d) => match s.section {
                Section::Copy => {
                    let i = CopyMode::ALL.iter().position(|&m| m == s.copy_mode).unwrap_or(0) as i32;
                    s.copy_mode = CopyMode::ALL[(i + d).rem_euclid(CopyMode::ALL.len() as i32) as usize];
                }
                Section::Tools => {
                    let i = ToolMode::ALL.iter().position(|&m| m == s.tool_mode).unwrap_or(0) as i32;
                    s.tool_mode = ToolMode::ALL[(i + d).rem_euclid(ToolMode::ALL.len() as i32) as usize];
                }
            },
            Act::StartAdj(d) => s.start = (s.start as i32 + d).clamp(0, s.end as i32) as u16,
            Act::EndAdj(d) => s.end = (s.end as i32 + d).clamp(s.start as i32, MAX_CYL as i32) as u16,
            Act::Side => s.side = s.side.next(),
            Act::Dens => s.hd = !s.hd,
            Act::Flow => s.flow = s.flow.next(),
            Act::ToolDevice => s.tool_adf = !s.tool_adf,
            Act::Fs => s.ffs = !s.ffs,
            Act::EditName => self.open_name(),
            Act::Source(i) => {
                if s.bus.drive(i).is_some() {
                    s.source = i;
                }
            }
            Act::Target(i) => {
                if s.bus.drive(i).is_some() {
                    s.targets[i] = !s.targets[i];
                }
            }
        }
    }

    // ───────────────────────────── drawing ─────────────────────────────
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_detect();
        self.poll_pick();
        let animating = self.eng.tick();
        if let Some(snd) = self.eng.sound.take() {
            if self.s.sound {
                crate::sound::play(snd);
            }
        }
        if let Some(name) = self.eng.probe_name.take() {
            // DF>ADF: the volume name is known, now ask where to save the image.
            if self.s.section == Section::Copy && self.s.flow == Flow::DiskToAdf {
                let nibble = self.s.copy_mode == CopyMode::Nibble;
                let ext = if nibble { "scp" } else { "adf" };
                let stem = name.as_deref().map(safe_file_stem).unwrap_or_else(|| "disk".into());
                let what = if nibble { "SCP flux image" } else { "Amiga disk file" };
                match self.auto_save.take() {
                    // REPEAT: same folder as the previous image, no file requester.
                    Some(dir) => {
                        let dir = dir.or_else(|| self.start_dir()).unwrap_or_else(|| PathBuf::from("."));
                        self.read_to_file(unique_path(&dir, &stem, ext));
                    }
                    None => self.spawn_dialog(Pick::ReadTo, Some((format!("{stem}.{ext}"), ext, what))),
                }
            }
        }
        if self.modal.is_none() {
            if let Some(a) = self.eng.ask.clone() {
                let buttons = if let Some((y, n)) = a.choice {
                    vec![y, n]
                } else if a.notice {
                    vec!["OK"]
                } else if a.yes_no {
                    vec!["YES", "NO"]
                } else {
                    vec!["OK", "CANCEL"]
                };
                self.modal = Some(Modal::new(ModalKind::Engine, &a.title, a.lines, buttons));
            }
        }
        if let Some((n, _)) = &self.pending_op {
            if self.eng.done_count >= *n {
                self.last_op = self.pending_op.take().map(|p| p.1);
            } else if !self.eng.busy() {
                self.pending_op = None;
            }
        }
        if self.eng.status != self.status_seen {
            self.status_seen = self.eng.status.clone();
            self.status_since = Instant::now();
        }

        // keyboard
        if self.modal.is_some() {
            self.modal_keys(&ctx);
        } else {
            let (enter, esc, f2) = ctx
                .input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape), i.key_pressed(Key::F2)));
            if esc {
                self.eng.stop();
            } else if enter {
                self.start();
            } else if f2 {
                self.repeat();
            }
        }

        let screen = ui.max_rect();
        if screen.width() < 2.0 || screen.height() < 2.0 {
            // minimised: nothing to draw
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        }
        let painter = ui.painter().clone();
        painter.rect_filled(screen, 0.0, pal::BG);
        let mut acts = Vec::new();
        let mut help = None;
        {
            let g = Gfx {
                p: painter,
                c: Canvas::fit(screen),
                font: &self.font,
                dots: &self.dots,
                t: self.t0.elapsed().as_secs_f64(),
            };
            self.draw(&g, ui, &mut acts, &mut help);
            self.draw_modal(&g, ui, &mut acts);
        }
        self.help = help;
        for a in acts {
            self.apply(a);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(if animating || self.modal.is_some() {
            33
        } else {
            80
        }));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, &self.s);
    }

    fn on_exit(&mut self) {
        crate::sound::cleanup();
        self.eng.stop();
        for t in &self.temps {
            let _ = std::fs::remove_file(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_names_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("xcopy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_path(&dir, "Workbench", "adf");
        assert_eq!(first, dir.join("Workbench.adf"));
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&dir, "Workbench", "adf"), dir.join("Workbench_2.adf"));
        std::fs::write(dir.join("Workbench_2.adf"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "Workbench", "adf"), dir.join("Workbench_3.adf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shown_names_fit_the_screen() {
        assert_eq!(short_name(Path::new("/x/game.adf"), 20), "GAME.ADF");
        let long = short_name(Path::new("/x/a_very_long_game_name_disk1.adf"), 20);
        assert_eq!(long.chars().count(), 20);
        assert!(long.ends_with(".."));
    }
}
