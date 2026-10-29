// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! X-COPY Modern — retro Amiga disk copier, formatter and writer for the Greaseweazle.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod settings;
mod sound;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("X-COPY Modern")
            .with_app_id("xcopy-modern")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([640.0, 360.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../../packaging/icon-256.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    eframe::run_native("xcopy-modern", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
