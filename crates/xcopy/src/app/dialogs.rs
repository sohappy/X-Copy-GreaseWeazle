// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! File dialogs (asynchronous, never blocking the window) and requesters (setup, volume
//! name, log, questions asked by the engine).

use super::*;
use crate::engine::{Level, Task};
use eframe::egui::{self, Key};
use hgf_core::fs::amigados;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

impl App {
    pub(super) fn start_dir(&self) -> Option<PathBuf> {
        self.s.last_dir.clone().filter(|d| d.is_dir())
    }

    /// Open a file dialog without blocking the window: the window keeps repainting and can be
    /// resized or moved while the dialog is open (a blocking dialog inside the frame made
    /// winit crash on resize). The result is collected by [`App::poll_pick`].
    pub(super) fn spawn_dialog(&mut self, purpose: Pick, save: Option<(String, &'static str, &'static str)>) {
        if self.picking.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        // Debug builds only: XCOPY_TEST_PICK=file answers every file dialog (UI tests).
        #[cfg(debug_assertions)]
        if let Some(p) = std::env::var_os("XCOPY_TEST_PICK") {
            let _ = tx.send(Some(PathBuf::from(p)));
            self.picking = Some((purpose, rx));
            return;
        }
        let mut d = rfd::AsyncFileDialog::new();
        if let Some(dir) = self.start_dir() {
            d = d.set_directory(dir);
        }
        match save {
            Some((name, ext, what)) => {
                let fut = d.set_file_name(name).add_filter(what, &[ext]).save_file();
                std::thread::spawn(move || {
                    let _ = tx.send(pollster::block_on(fut).map(|h| h.path().to_path_buf()));
                });
            }
            None => {
                let d = if matches!(purpose, Pick::KickRom) {
                    d.add_filter("Kickstart ROM image", &["rom", "bin", "a1000", "kick"])
                } else {
                    d.add_filter("Amiga disk image", &["adf", "adz", "scp", "ipf", "hfe", "raw", "dmk"])
                };
                let fut = d.add_filter("All files", &["*"]).pick_file();
                std::thread::spawn(move || {
                    let _ = tx.send(pollster::block_on(fut).map(|h| h.path().to_path_buf()));
                });
            }
        }
        self.picking = Some((purpose, rx));
        self.eng.set_status("SELECT A FILE...", Level::Warn);
    }

    pub(super) fn poll_pick(&mut self) {
        let Some((_, rx)) = &self.picking else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        let Some((purpose, _)) = self.picking.take() else { return };
        let Some(path) = result else {
            self.eng.set_status("NO FILE SELECTED.", Level::Warn);
            return;
        };
        self.s.last_dir = path.parent().map(Path::to_path_buf);
        match purpose {
            Pick::ReadTo => self.read_to_file(path),
            Pick::WriteFrom => self.write_from_file(path),
            Pick::SaveBlank(image) => match std::fs::write(&path, &image) {
                Ok(()) => self.eng.set_status(
                    format!(
                        "BLANK ADF SAVED: {}",
                        path.file_name().unwrap_or_default().to_string_lossy().to_uppercase()
                    ),
                    Level::Good,
                ),
                Err(e) => self.notice("SAVE FAILED", &e.to_string()),
            },
            Pick::KickRom => self.kick_disk_from(path),
            Pick::Dir => {
                self.launch(vec![Task::Dir { path }, Task::Done { message: "OPERATION DONE.".into() }])
            }
        }
    }

    pub(super) fn open_setup(&mut self) {
        let mut m = Modal::new(
            ModalKind::Setup,
            "SETUP",
            vec![("Click BUS to change how the drives are wired.".into(), Level::Info)],
            vec!["DETECT", "OK", "CANCEL"],
        );
        m.fields = vec![
            Field { label: "GW PROGRAM (HOST TOOLS)", value: self.s.cfg.program.clone(), max: 400 },
            Field { label: "SERIAL PORT (EMPTY = AUTO)", value: self.s.cfg.device.clone(), max: 120 },
        ];
        m.bus = self.s.bus;
        m.sound = self.s.sound;
        self.modal = Some(m);
    }

    pub(super) fn open_name(&mut self) {
        let mut m = Modal::new(
            ModalKind::Name,
            "VOLUME NAME",
            vec![("Name of the formatted disk (max 30 chars).".into(), Level::Info)],
            vec!["OK", "CANCEL"],
        );
        m.fields = vec![Field { label: "NAME", value: self.s.volume.clone(), max: 30 }];
        self.modal = Some(m);
    }

    pub(super) fn open_log(&mut self) {
        let mut m = Modal::new(ModalKind::Log, "LOG", Vec::new(), vec!["OK", "CLEAR"]);
        m.scroll = usize::MAX;
        self.modal = Some(m);
    }

    pub(super) fn modal_button(&mut self, i: usize) {
        let Some(m) = self.modal.as_mut() else { return };
        let label = m.buttons.get(i).copied().unwrap_or("");
        match m.kind {
            ModalKind::Engine => {
                // first button = OK / YES / REPLACE
                let yes = i == 0 && label != "CANCEL" && label != "NO";
                self.modal = None;
                self.eng.answer(yes);
            }
            ModalKind::Log => {
                if label == "CLEAR" {
                    self.eng.log.clear();
                } else {
                    self.modal = None;
                }
            }
            ModalKind::Name => {
                if label == "OK" {
                    let v = m.fields[0].value.trim().to_string();
                    match amigados::validate_name(&v) {
                        Ok(()) => {
                            self.s.volume = v;
                            self.modal = None;
                        }
                        Err(e) => m.error = Some(e.to_uppercase()),
                    }
                } else {
                    self.modal = None;
                }
            }
            ModalKind::Setup => match label {
                "DETECT" => {
                    let p = m.fields[0].value.trim().to_string();
                    m.error = Some("SEARCHING...".into());
                    self.s.cfg.program = if p.is_empty() { "gw".into() } else { p };
                    self.detect();
                }
                "OK" => {
                    let p = m.fields[0].value.trim().to_string();
                    let changed = p != self.s.cfg.program;
                    self.s.cfg.program = if p.is_empty() { "gw".into() } else { p };
                    self.s.cfg.device = m.fields[1].value.trim().to_string();
                    self.s.bus = m.bus;
                    self.s.sound = m.sound;
                    for i in 0..4 {
                        if self.s.bus.drive(i).is_none() {
                            self.s.targets[i] = false;
                        }
                    }
                    if self.s.bus.drive(self.s.source).is_none() {
                        self.s.source = 0;
                    }
                    if !self.s.targets.iter().any(|&t| t) {
                        self.s.targets[0] = true;
                    }
                    self.modal = None;
                    if changed {
                        self.detect();
                    }
                }
                _ => self.modal = None,
            },
        }
    }

    pub(super) fn modal_keys(&mut self, ctx: &egui::Context) {
        let Some(m) = self.modal.as_mut() else { return };
        let events = ctx.input(|i| i.events.clone());
        let mut press: Option<usize> = None;
        for e in events {
            match e {
                egui::Event::Text(t) if !m.fields.is_empty() => {
                    let f = &mut m.fields[m.focus];
                    for ch in t.chars() {
                        if f.value.chars().count() < f.max && (ch as u32) >= 32 && (ch as u32) <= 255 {
                            f.value.push(ch);
                        }
                    }
                    m.error = None;
                }
                egui::Event::Paste(t) if !m.fields.is_empty() => {
                    let f = &mut m.fields[m.focus];
                    for ch in t.trim().chars() {
                        if f.value.chars().count() < f.max {
                            f.value.push(ch);
                        }
                    }
                }
                egui::Event::Key { key, pressed: true, .. } => match key {
                    Key::Backspace if !m.fields.is_empty() => {
                        m.fields[m.focus].value.pop();
                    }
                    Key::Tab if !m.fields.is_empty() => m.focus = (m.focus + 1) % m.fields.len(),
                    Key::Enter => {
                        press = Some(m.buttons.iter().position(|b| matches!(*b, "OK" | "YES")).unwrap_or(0));
                    }
                    Key::Escape => {
                        press = Some(
                            m.buttons
                                .iter()
                                .position(|b| matches!(*b, "CANCEL" | "NO"))
                                .unwrap_or(m.buttons.len().saturating_sub(1)),
                        );
                        if m.kind == ModalKind::Log {
                            press = Some(0);
                        }
                    }
                    Key::ArrowDown | Key::PageDown => {
                        m.scroll = m.scroll.saturating_add(if key == Key::PageDown { 10 } else { 1 })
                    }
                    Key::ArrowUp | Key::PageUp => {
                        m.scroll = m.scroll.saturating_sub(if key == Key::PageUp { 10 } else { 1 })
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if let Some(i) = press {
            self.modal_button(i);
        }
    }
}
