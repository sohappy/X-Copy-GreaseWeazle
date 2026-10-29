// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Drawing of the X-COPY screen and its requesters; clicks are returned as [`Act`]s.

use super::*;
use crate::engine::Level;
use crate::settings::*;
use crate::ui::font::{wrap, Style};
use crate::ui::gfx::{self, lerp, pal, Button, Gfx};
use eframe::egui::{self, Color32, CursorIcon, Id, Rect, Sense, Ui};
use hgf_core::TrackState;

impl App {
    pub(super) fn draw(&self, g: &Gfx, ui: &Ui, acts: &mut Vec<Act>, help: &mut Option<String>) {
        let live = self.modal.is_none();
        let busy = self.eng.busy() || self.picking.is_some();
        let hot =
            |id: &str, x: f32, y: f32, w: f32, h: f32| -> Hot { Hot::new(ui, live, id, g.c.r(x, y, w, h)) };

        g.fill(0.0, 0.0, gfx::W, gfx::H, pal::BG);

        // ── title band ──
        g.bead_frame(8.0, 8.0, 262.0, 182.0);
        g.text_centered(139.0, 36.0, 3.0, "\u{a9} 2026", Style::grad(pal::WHITE, pal::CYAN_DARK));
        g.text_centered(
            139.0,
            80.0,
            4.4,
            "RCC(A)",
            Style::grad(Color32::from_rgb(255, 244, 80), Color32::from_rgb(226, 36, 16)),
        );
        g.text_centered(139.0, 142.0, 2.0, "EDITION 1.0", Style::shadowed(pal::CYAN));

        g.fill(278.0, 8.0, 724.0, 182.0, pal::PANEL);
        g.dotted(282.0, 12.0, 716.0, 174.0);
        g.stroke(278.0, 8.0, 724.0, 182.0, 3.0, pal::LINE);
        gfx::logo(g, 640.0 - 6.0 * 9.0 * 12.0 / 2.0, 16.0, "X-COPY", 12.0);
        g.text2(
            640.0 - 20.0 * 8.0 * 3.2 / 2.0,
            124.0,
            3.2,
            4.4,
            "Retro Computing Club",
            Style::grad(Color32::from_rgb(240, 250, 255), Color32::from_rgb(60, 118, 146)),
        );
        g.text_centered(
            640.0,
            172.0,
            1.5,
            "MODERN  \u{b7}  GREASEWEAZLE POWERED",
            Style::flat(pal::CYAN_DARK),
        );

        g.bead_frame(1010.0, 8.0, 262.0, 182.0);
        let (gw_line, gw_level) = match (self.gw_found, self.eng.device_ok) {
            (None, _) => ("SEARCHING GW", Level::Warn),
            (Some(false), _) => ("GW NOT FOUND", Level::Error),
            (Some(true), Some(true)) => ("DEVICE OK", Level::Good),
            (Some(true), Some(false)) => ("NO DEVICE", Level::Error),
            (Some(true), None) => ("GW READY", Level::Good),
        };
        g.text_centered(1141.0, 34.0, 2.0, "GREASEWEAZLE", Style::shadowed(pal::CYAN));
        g.text_centered(1141.0, 60.0, 2.0, gw_line, Style::shadowed(level_color(gw_level)));
        let model: String = if self.eng.model.is_empty() {
            if self.s.bus == Bus::Ibm {
                "BUS: IBM PC".into()
            } else {
                "BUS: SHUGART".into()
            }
        } else {
            self.eng.model.to_uppercase().replace("GREASEWEAZLE ", "GW ").chars().take(13).collect()
        };
        g.text_centered(1141.0, 86.0, 2.0, &model, Style::shadowed(pal::CYAN));
        let h = hot("setup", 1050.0, 112.0, 182.0, 50.0);
        g.button(
            1050.0,
            112.0,
            182.0,
            50.0,
            &Button { label: "SETUP", color: pal::YELLOW, hover: h.hover, pressed: h.down },
        );
        if h.hover {
            *help = Some("SETUP: GW PROGRAM, SERIAL PORT AND DRIVE BUS".into());
        }
        if h.click {
            acts.push(Act::Setup);
        }

        // ── option bar ──
        g.line_frame(8.0, 198.0, 1264.0, 120.0);
        g.fill(14.0, 240.0, 1252.0, 2.0, pal::LINE_DIM);
        let val = Style::grad(pal::YELLOW_TOP, pal::YELLOW_BOT);
        let dim = Style::shadowed(Color32::from_rgb(70, 76, 60));
        let vs = 4.0;
        let vy = 262.0;
        let header = |x: f32,
                      label: &str,
                      knob: bool,
                      active: bool,
                      id: &str,
                      a: Act,
                      tip: &str,
                      acts: &mut Vec<Act>,
                      help: &mut Option<String>| {
            let tw = crate::ui::font::Font::width(label, 2.4) + if knob { 18.0 } else { 0.0 };
            let h = hot(id, x - 4.0, 208.0, tw + 8.0, 28.0);
            if knob {
                g.knob(x + 6.0, 221.0, h.hover);
            }
            let tx = if knob { x + 18.0 } else { x };
            let col = if active {
                pal::YELLOW
            } else if h.hover {
                pal::WHITE
            } else {
                pal::CYAN
            };
            g.text(tx, 212.0, 2.4, label, Style::shadowed(col));
            if knob && h.hover {
                *help = Some(tip.into());
            }
            if knob && h.click {
                acts.push(a);
            }
        };
        let copy = self.s.section == Section::Copy;
        header(
            24.0,
            "COPY",
            true,
            copy,
            "h-copy",
            Act::Section(Section::Copy),
            "COPY: CLICK TO CHOOSE THE COPY MODE",
            acts,
            help,
        );
        header(
            150.0,
            "TOOLS",
            true,
            !copy,
            "h-tools",
            Act::Section(Section::Tools),
            "TOOLS: FORMAT, QFORMAT, ERASE, SPEEDCHK, CLEAN",
            acts,
            help,
        );
        let mode_label = if copy { self.s.copy_mode.label() } else { self.s.tool_mode.label() };
        let h = hot("mode", 20.0, 252.0, 290.0, 52.0);
        g.text(28.0, vy, vs, mode_label, val);
        if h.hover {
            *help = Some(if copy { self.s.copy_mode.help() } else { self.s.tool_mode.help() }.into());
        }
        if h.click {
            acts.push(Act::Mode(1));
        }
        if h.rclick {
            acts.push(Act::Mode(-1));
        }

        let tool = self.s.tool_mode;
        let range_active = copy || tool == ToolMode::Erase;
        let format_tool = !copy && tool.is_format();
        let track0 = !copy && tool == ToolMode::Track0;
        let kick = !copy && tool == ToolMode::KickDisk;
        // number with arrows
        let spin = |x: f32,
                    id: &str,
                    v: u16,
                    head: u8,
                    a: fn(i32) -> Act,
                    enabled: bool,
                    acts: &mut Vec<Act>,
                    help: &mut Option<String>| {
            let st = if enabled { val } else { dim };
            g.text(x, vy, vs, &format!("{v:02}"), st);
            g.text(x + 84.0, vy, vs, &head.to_string(), st);
            if !enabled {
                return;
            }
            let up = hot(&format!("{id}-up"), x + 8.0, 244.0, 48.0, 16.0);
            let dn = hot(&format!("{id}-dn"), x + 8.0, 296.0, 48.0, 16.0);
            g.arrow(x + 16.0, 252.0, true, up.hover);
            g.arrow(x + 48.0, 252.0, true, up.hover);
            g.arrow(x + 16.0, 304.0, false, dn.hover);
            g.arrow(x + 48.0, 304.0, false, dn.hover);
            let wheel = hot(&format!("{id}-num"), x, vy - 4.0, 100.0, 40.0);
            if up.hover || dn.hover || wheel.hover {
                *help = Some("CYLINDER: CLICK THE ARROWS (RIGHT-CLICK = 10 STEPS) OR SCROLL".into());
            }
            for (h, d) in [(&up, 1), (&dn, -1)] {
                if h.click {
                    acts.push(a(d));
                }
                if h.rclick {
                    acts.push(a(d * 10));
                }
            }
            if wheel.hover && wheel.scroll != 0 {
                acts.push(a(wheel.scroll));
            }
        };
        let cycle = |x: f32,
                     w: f32,
                     id: &str,
                     label: &str,
                     value: &str,
                     enabled: bool,
                     a: Act,
                     tip: &str,
                     acts: &mut Vec<Act>,
                     help: &mut Option<String>| {
            header(x, label, enabled, false, &format!("{id}-h"), a, tip, acts, help);
            g.text(x, vy, vs, value, if enabled { val } else { dim });
            if enabled {
                let h = hot(id, x - 4.0, 252.0, w, 52.0);
                if h.hover {
                    *help = Some(tip.into());
                }
                if h.click {
                    acts.push(a);
                }
            }
        };

        if kick {
            header(330.0, "CYLINDERS", false, false, "h-kcyl", Act::Start, "", acts, help);
            g.text(330.0, vy, vs, "00-23", val);
            header(640.0, "ROM", false, false, "h-krom", Act::Start, "", acts, help);
            g.text(640.0, vy, vs, "256K", val);
        } else if track0 {
            header(330.0, "TRACK", false, false, "h-trk0", Act::Start, "", acts, help);
            g.text(330.0, vy, vs, "00", val);
            g.text(414.0, vy + 6.0, 3.0, "BOTH", val);
            cycle(
                640.0,
                190.0,
                "fs",
                "FS",
                if self.s.ffs { "FFS" } else { "OFS" },
                true,
                Act::Fs,
                "FS: MUST MATCH THE DISK (OFS = KICK 1.X, FFS = KICK 2.0+)",
                acts,
                help,
            );
        } else if format_tool {
            header(
                330.0,
                "NAME",
                true,
                false,
                "h-name",
                Act::EditName,
                "NAME: CLICK TO EDIT THE VOLUME NAME",
                acts,
                help,
            );
            let name: String = self.s.volume.to_uppercase().chars().take(11).collect();
            let more = if self.s.volume.chars().count() > 11 { "\u{bb}" } else { "" };
            g.text(330.0, vy + 6.0, 3.0, &format!("{name}{more}"), val);
            let h = hot("name", 326.0, 252.0, 300.0, 52.0);
            if h.hover {
                *help = Some("NAME: CLICK TO EDIT THE VOLUME NAME".into());
            }
            if h.click {
                acts.push(Act::EditName);
            }
            cycle(
                640.0,
                190.0,
                "fs",
                "FS",
                if self.s.ffs { "FFS" } else { "OFS" },
                true,
                Act::Fs,
                "FS: OFS = ALL AMIGAS (KICK 1.X), FFS = KICKSTART 2.0+",
                acts,
                help,
            );
        } else {
            header(330.0, "START", false, false, "h-start", Act::Start, "", acts, help);
            header(480.0, "END", false, false, "h-end", Act::Start, "", acts, help);
            spin(330.0, "start", self.s.start, self.s.start_head(), Act::StartAdj, range_active, acts, help);
            spin(480.0, "end", self.s.end, self.s.end_head(), Act::EndAdj, range_active, acts, help);
            cycle(
                640.0,
                190.0,
                "side",
                "SIDE",
                self.s.side.label(),
                range_active,
                Act::Side,
                "SIDE: BOTH, UPPER (HEAD 1) OR LOWER (HEAD 0)",
                acts,
                help,
            );
        }
        let dens_on = copy && self.s.copy_mode != CopyMode::Nibble || format_tool || track0;
        cycle(
            840.0,
            140.0,
            "dens",
            "DENS",
            if self.s.hd { "HD" } else { "DD" },
            dens_on,
            Act::Dens,
            "DENS: DD 880K OR HD 1.76M",
            acts,
            help,
        );
        if copy {
            cycle(
                990.0,
                260.0,
                "flow",
                "DEVICE",
                self.s.flow.label(),
                true,
                Act::Flow,
                self.s.flow.help(),
                acts,
                help,
            );
        } else if format_tool || kick {
            let v = if self.s.tool_adf { "ADF" } else { "DISK" };
            cycle(
                990.0,
                260.0,
                "tdev",
                "DEVICE",
                v,
                true,
                Act::ToolDevice,
                if kick {
                    "DEVICE: WRITE THE KICKSTART DISK OR SAVE IT AS AN ADF"
                } else {
                    "DEVICE: FORMAT THE DISK(S) OR SAVE A BLANK ADF FILE"
                },
                acts,
                help,
            );
        } else {
            cycle(990.0, 260.0, "tdev", "DEVICE", "DISK", false, Act::ToolDevice, "", acts, help);
        }

        // ── buttons ──
        let bw = (1264.0 - 6.0 * 8.0) / 7.0;
        let buttons: [(&str, Color32, Act, &str); 7] = [
            ("START", pal::YELLOW, Act::Start, "START THE SELECTED OPERATION (ENTER)"),
            ("REPEAT", pal::YELLOW, Act::Repeat, "REPEAT: WRITE THE LAST IMAGE AGAIN TO NEW DISKS (F2)"),
            ("STOP", pal::YELLOW, Act::Stop, "STOP: INTERRUPT AND RELEASE THE DRIVE (ESC)"),
            ("DISK INFO", pal::GREEN, Act::DiskInfo, "DISK INFO: GREASEWEAZLE MODEL, FIRMWARE, PORT"),
            ("DIRECTORY", pal::GREEN, Act::Directory, "DIRECTORY: SHOW THE AMIGADOS FILES OF THE SOURCE"),
            ("CHECKDISK", pal::GREEN, Act::CheckDisk, "CHECKDISK: QUICK READ OF THE SOURCE, TRACK MAP"),
            ("DEFAULT", pal::RED, Act::Default, "DEFAULT: RESET ALL COPY OPTIONS"),
        ];
        for (i, (label, col, a, tip)) in buttons.iter().enumerate() {
            let x = 8.0 + i as f32 * (bw + 8.0);
            let h = hot(&format!("btn{i}"), x, 326.0, bw, 40.0);
            let enabled = match a {
                Act::Stop => busy,
                _ => !busy,
            };
            let c = if enabled { *col } else { lerp(*col, Color32::BLACK, 0.55) };
            g.button(
                x,
                326.0,
                bw,
                40.0,
                &Button { label, color: c, hover: h.hover && enabled, pressed: h.down && enabled },
            );
            if h.hover {
                *help = Some((*tip).into());
            }
            if h.click && enabled {
                acts.push(*a);
            }
        }

        // ── drives ──
        g.line_frame(8.0, 374.0, 500.0, 274.0);
        g.text_centered(258.0, 384.0, 2.4, "SOURCE DRIVES", Style::shadowed(pal::CYAN));
        g.text_centered(258.0, 626.0, 2.4, "TARGET DRIVES", Style::shadowed(pal::CYAN));
        let uses_source = (copy && self.s.flow != Flow::AdfToDisk) || (!copy && tool == ToolMode::Speed);
        let uses_target = (copy && self.s.flow != Flow::DiskToAdf)
            || (!copy && tool != ToolMode::Speed && !((format_tool || kick) && self.s.tool_adf));
        let blink = (g.t * 6.0).sin() > 0.0;
        for i in 0..4 {
            let x = 24.0 + i as f32 * 120.0;
            let enabled = self.s.bus.drive(i).is_some();
            let cx = x + 52.0;
            let hs = hot(&format!("src{i}"), cx - 20.0, 398.0, 40.0, 40.0);
            g.bulb(cx, 412.0, uses_source && self.s.source == i && enabled, enabled && uses_source);
            let ht = hot(&format!("tgt{i}"), cx - 20.0, 568.0, 40.0, 44.0);
            g.bulb(cx, 580.0, uses_target && self.s.targets[i] && enabled, enabled && uses_target);
            let led = match self.eng.active {
                Some((d, w)) if d == i && self.eng.running_gw() => Some(if w {
                    if blink {
                        pal::RED
                    } else {
                        pal::RED_DARK
                    }
                } else {
                    pal::GREEN
                }),
                _ => None,
            };
            let hd = hot(&format!("drv{i}"), x, 444.0, 104.0, 96.0);
            g.floppy(x, 444.0, i, enabled, led, hd.hover);
            if !enabled && (hs.hover || ht.hover || hd.hover) {
                *help = Some(format!("DF{i} IS NOT AVAILABLE ON THE IBM PC BUS (SEE SETUP)"));
            } else if hs.hover && uses_source {
                *help = Some(format!("SOURCE: READ FROM DF{i}"));
            } else if ht.hover && uses_target {
                *help = Some(format!("TARGET: CLICK TO ADD/REMOVE DF{i} (SEVERAL ALLOWED)"));
            } else if hd.hover {
                let d = self.s.bus.drive(i).map(|d| d.label()).unwrap_or("");
                *help = Some(format!("DF{i} = {}", d.to_uppercase()));
            }
            if hs.click && uses_source {
                acts.push(Act::Source(i));
            }
            if ht.click && uses_target {
                acts.push(Act::Target(i));
            }
        }

        // ── track maps ──
        let c = self.eng.map.counts();
        let footer_l = format!("OK:{:03} BAD:{:03}", c.ok + c.recovered, c.bad);
        let secs = self.eng.elapsed().as_secs();
        let footer_r = format!("TIME: {:02}:{:02}", secs / 60, secs % 60);
        self.grid(g, 516.0, "UPPER SIDE", 1, &footer_l, blink);
        self.grid(g, 898.0, "LOWER SIDE", 0, &footer_r, blink);

        // ── status line ──
        g.line_frame(8.0, 656.0, 1264.0, 56.0);
        let hs = hot("status", 8.0, 656.0, 1264.0, 56.0);
        if hs.click {
            acts.push(Act::Log);
        }
        let (text, lvl) = if !busy && live {
            match help {
                Some(h) => (h.clone(), Level::Cmd),
                None if hs.hover => ("CLICK HERE TO SHOW THE LOG.".to_string(), Level::Cmd),
                None => (self.eng.status.clone(), self.eng.level),
            }
        } else {
            (self.eng.status.clone(), self.eng.level)
        };
        let size = 3.0;
        let max = ((1264.0 - 40.0) / (8.0 * size)) as usize;
        let typed = if text == self.eng.status && !self.eng.running_gw() {
            ((self.status_since.elapsed().as_secs_f32() * 90.0) as usize).max(1)
        } else {
            usize::MAX
        };
        let shown: String = if text.chars().count() > max {
            let pad = format!("{text}     ");
            let n = pad.chars().count();
            let off = ((g.t * 8.0) as usize) % n;
            pad.chars().cycle().skip(off).take(max).collect()
        } else {
            text.chars().take(typed).collect()
        };
        let col = level_color(lvl);
        g.text(28.0, 672.0, size, &shown, Style::grad(lerp(col, Color32::WHITE, 0.35), col));
        if busy && self.eng.running_gw() {
            // progress bar under the text
            let p = self.eng.map.progress().clamp(0.0, 1.0);
            g.fill(28.0, 702.0, 1224.0, 3.0, pal::GREEN_DARK);
            g.fill(28.0, 702.0, 1224.0 * p, 3.0, pal::GREEN);
        }

        // tooltip of the hovered track cell
        if live {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                let v = g.c.inv(pos);
                for (x0, head) in [(516.0, 1u8), (898.0, 0u8)] {
                    if let Some(cyl) = cell_at(x0, v.x, v.y) {
                        self.cell_tooltip(g, v.x, v.y, cyl, head);
                    }
                }
            }
        }
    }

    pub(super) fn grid(&self, g: &Gfx, x: f32, title: &str, head: u8, footer: &str, blink: bool) {
        g.line_frame(x, 374.0, 374.0, 274.0);
        g.text_centered(x + 187.0, 384.0, 2.4, title, Style::shadowed(pal::CYAN));
        let (gx, gy, cw, ch) = grid_geo(x);
        for col in 0..10 {
            g.text_centered(
                gx + col as f32 * cw + cw / 2.0,
                406.0,
                2.0,
                &col.to_string(),
                Style::flat(pal::CYAN),
            );
        }
        let map = &self.eng.map;
        let in_head = map.geometry.heads.contains(&head);
        for row in 0..9u16 {
            g.text(x + 12.0, gy + row as f32 * ch + 4.0, 2.0, &row.to_string(), Style::flat(pal::CYAN));
            for col in 0..10u16 {
                let cyl = row * 10 + col;
                if cyl > MAX_CYL {
                    continue;
                }
                let (cx, cy) = (gx + col as f32 * cw, gy + row as f32 * ch);
                let inside = in_head && map.geometry.cyls.contains(&cyl);
                let info = map.get(cyl, head);
                let active = map.active == Some((cyl, head));
                let (bg, border, glyph, fg) =
                    cell_look(inside, info.state, info.sectors, active, blink, map.mode);
                g.fill(cx + 1.0, cy + 1.0, cw - 3.0, ch - 3.0, bg);
                g.stroke(cx + 1.0, cy + 1.0, cw - 3.0, ch - 3.0, 1.5, border);
                if !glyph.is_empty() {
                    g.text_centered(cx + cw / 2.0 - 1.0, cy + (ch - 16.0) / 2.0, 2.0, glyph, Style::flat(fg));
                }
            }
        }
        g.text_centered(x + 187.0, 624.0, 2.0, footer, Style::shadowed(pal::CYAN));
    }

    pub(super) fn cell_tooltip(&self, g: &Gfx, mx: f32, my: f32, cyl: u16, head: u8) {
        let map = &self.eng.map;
        let info = map.get(cyl, head);
        let inside = map.geometry.heads.contains(&head) && map.geometry.cyls.contains(&cyl);
        let mut lines =
            vec![format!("TRACK {cyl:02} SIDE {head} ({})", if head == 1 { "UPPER" } else { "LOWER" })];
        if inside {
            lines.push(info.state.label().to_uppercase());
            if let Some((ok, tot)) = info.sectors {
                lines.push(format!("SECTORS {ok}/{tot}"));
            }
            if info.retries > 0 {
                lines.push(format!("RETRIES {}", info.retries));
            }
            if !info.sector_map.is_empty() {
                let s: String = info
                    .sector_map
                    .iter()
                    .map(|s| match s {
                        Some(true) => 'o',
                        Some(false) => 'X',
                        None => '.',
                    })
                    .collect();
                lines.push(s);
            }
        } else {
            lines.push("NOT IN THIS OPERATION".into());
        }
        let w = lines.iter().map(|l| l.len()).max().unwrap_or(0) as f32 * 12.8 + 20.0;
        let h = lines.len() as f32 * 16.0 + 14.0;
        let x = (mx + 18.0).min(gfx::W - w - 6.0);
        let y = (my - h - 8.0).max(4.0);
        g.fill(x, y, w, h, Color32::from_rgb(4, 6, 14));
        g.stroke(x, y, w, h, 2.0, pal::YELLOW);
        for (i, l) in lines.iter().enumerate() {
            g.text(
                x + 10.0,
                y + 8.0 + i as f32 * 16.0,
                1.6,
                l,
                Style::flat(if i == 0 { pal::YELLOW } else { pal::CYAN }),
            );
        }
    }

    pub(super) fn draw_modal(&self, g: &Gfx, ui: &Ui, acts: &mut Vec<Act>) {
        let Some(m) = &self.modal else { return };
        let hot =
            |id: &str, x: f32, y: f32, w: f32, h: f32| -> Hot { Hot::new(ui, true, id, g.c.r(x, y, w, h)) };
        // swallow clicks on the background
        let _ = hot("modal-bg", 0.0, 0.0, gfx::W, gfx::H);
        g.p.rect_filled(g.c.r(0.0, 0.0, gfx::W, gfx::H), 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 170));

        let log = m.kind == ModalKind::Log;
        let size = if log { 1.25 } else { 2.2 };
        let wbox = if log { 1100.0 } else { 820.0 };
        let cols = ((wbox - 64.0) / (8.0 * size)) as usize;
        let mut lines: Vec<(String, Level)> = Vec::new();
        let src: Vec<(String, Level)> = if log { self.eng.log.clone() } else { m.lines.clone() };
        for (l, lv) in &src {
            for w in wrap(l, cols) {
                lines.push((w, *lv));
            }
        }
        let visible = if log { 30 } else { 13 };
        let max_scroll = lines.len().saturating_sub(visible);
        let scroll = m.scroll.min(max_scroll);
        let shown = lines.len().min(visible);
        let lh = 8.0 * size + 6.0;
        let fields_h =
            m.fields.len() as f32 * 64.0 + 8.0 + if m.kind == ModalKind::Setup { 72.0 } else { 0.0 };
        let err_h = if m.error.is_some() { 28.0 } else { 0.0 };
        let hbox = 96.0 + shown as f32 * lh + fields_h + err_h + 70.0;
        let x = (gfx::W - wbox) / 2.0;
        let y = ((gfx::H - hbox) / 2.0).max(4.0);
        g.bead_frame(x, y, wbox, hbox);
        g.text_centered(
            x + wbox / 2.0,
            y + 30.0,
            3.0,
            &m.title,
            Style::grad(pal::YELLOW_TOP, pal::YELLOW_BOT),
        );
        let mut cy = y + 70.0;
        for (l, lv) in lines.iter().skip(scroll).take(visible) {
            g.text(x + 32.0, cy, size, l, Style::shadowed(level_color(*lv)));
            cy += lh;
        }
        if max_scroll > 0 {
            let track_h = shown as f32 * lh;
            let bar_h = (track_h * visible as f32 / lines.len() as f32).max(12.0);
            let by = y + 70.0 + (track_h - bar_h) * scroll as f32 / max_scroll as f32;
            g.fill(x + wbox - 28.0, y + 70.0, 6.0, track_h, pal::LINE_DIM);
            g.fill(x + wbox - 28.0, by, 6.0, bar_h, pal::CYAN);
            let area = hot("modal-scroll", x, y + 60.0, wbox, track_h + 20.0);
            if area.scroll != 0 {
                acts.push(Act::ModalScroll(-area.scroll * 3));
            }
        }
        if !m.fields.is_empty() {
            cy += 8.0;
        }
        for (i, f) in m.fields.iter().enumerate() {
            g.text(x + 32.0, cy + 4.0, 2.0, f.label, Style::shadowed(pal::CYAN));
            let (fx, fy, fw, fh) = (x + 32.0, cy + 24.0, wbox - 64.0, 32.0);
            let h = hot(&format!("field{i}"), fx, fy, fw, fh);
            g.fill(fx, fy, fw, fh, Color32::from_rgb(2, 4, 10));
            g.stroke(fx, fy, fw, fh, 2.0, if i == m.focus { pal::YELLOW } else { pal::LINE_DIM });
            let maxc = ((fw - 20.0) / 16.0) as usize - 1;
            let n = f.value.chars().count();
            let tail: String = f.value.chars().skip(n.saturating_sub(maxc)).collect();
            let cursor = if i == m.focus && (g.t * 2.5).fract() < 0.6 { "_" } else { "" };
            g.text(fx + 10.0, fy + 8.0, 2.0, &format!("{tail}{cursor}"), Style::flat(pal::YELLOW));
            if h.click {
                acts.push(Act::ModalField(i));
            }
            cy += 64.0;
        }
        if m.kind == ModalKind::Setup {
            let h = hot("bus", x + 32.0, cy, wbox - 64.0, 28.0);
            g.knob(x + 40.0, cy + 12.0, h.hover);
            g.text(
                x + 56.0,
                cy + 4.0,
                2.0,
                &format!("BUS: {}", m.bus.label()),
                Style::shadowed(if h.hover { pal::WHITE } else { pal::YELLOW }),
            );
            if h.click {
                acts.push(Act::Bus);
            }
            cy += 36.0;
            let h = hot("sound", x + 32.0, cy, wbox - 64.0, 28.0);
            g.knob(x + 40.0, cy + 12.0, h.hover);
            g.text(
                x + 56.0,
                cy + 4.0,
                2.0,
                &format!("SOUND: {}", if m.sound { "ON" } else { "OFF" }),
                Style::shadowed(if h.hover { pal::WHITE } else { pal::YELLOW }),
            );
            if h.click {
                acts.push(Act::SoundToggle);
            }
            cy += 36.0;
        }
        if let Some(e) = &m.error {
            g.text(x + 32.0, cy + 2.0, 2.0, e, Style::shadowed(pal::RED));
        }
        let n = m.buttons.len() as f32;
        let bw = 170.0;
        let gap = 24.0;
        let total = n * bw + (n - 1.0) * gap;
        let by = y + hbox - 60.0;
        for (i, b) in m.buttons.iter().enumerate() {
            let bx = x + (wbox - total) / 2.0 + i as f32 * (bw + gap);
            let h = hot(&format!("mbtn{i}"), bx, by, bw, 38.0);
            let col = match *b {
                "NO" | "CANCEL" => pal::RED,
                "DETECT" | "CLEAR" => pal::GREEN,
                _ => pal::YELLOW,
            };
            g.button(bx, by, bw, 38.0, &Button { label: b, color: col, hover: h.hover, pressed: h.down });
            if h.click {
                acts.push(Act::ModalButton(i));
            }
        }
    }
}

/// Grid origin and cell size of a track map panel at `x`.
fn grid_geo(x: f32) -> (f32, f32, f32, f32) {
    (x + 34.0, 424.0, 33.0, 21.5)
}

fn cell_at(x0: f32, vx: f32, vy: f32) -> Option<u16> {
    let (gx, gy, cw, ch) = grid_geo(x0);
    if vx < gx || vy < gy || vx >= gx + cw * 10.0 || vy >= gy + ch * 9.0 {
        return None;
    }
    let cyl = ((vy - gy) / ch) as u16 * 10 + ((vx - gx) / cw) as u16;
    (cyl <= MAX_CYL).then_some(cyl)
}

/// Colours and letter of one track cell.
fn cell_look(
    inside: bool,
    state: TrackState,
    sectors: Option<(u32, u32)>,
    active: bool,
    blink: bool,
    mode: hgf_core::MapMode,
) -> (Color32, Color32, &'static str, Color32) {
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    let empty_bg = Color32::from_rgb(6, 16, 12);
    if !inside {
        return (empty_bg, Color32::from_rgb(16, 46, 26), "", pal::GREY);
    }
    if active {
        return if blink {
            (pal::WHITE, pal::WHITE, if mode == hgf_core::MapMode::Read { "R" } else { "W" }, Color32::BLACK)
        } else {
            (pal::GREEN_BOX, pal::WHITE, "", pal::WHITE)
        };
    }
    match state {
        TrackState::Pending => (empty_bg, pal::GREEN_BOX, "", pal::GREEN),
        TrackState::Active => (pal::WHITE, pal::WHITE, "", Color32::BLACK),
        TrackState::Ok => {
            let g = if mode == hgf_core::MapMode::Erase { "E" } else { "O" };
            (pal::GREEN_DARK, pal::GREEN, g, pal::GREEN)
        }
        TrackState::Recovered => (Color32::from_rgb(70, 60, 0), pal::YELLOW, "R", pal::YELLOW),
        TrackState::Retrying => {
            let c = if blink { pal::ORANGE } else { Color32::from_rgb(90, 50, 0) };
            (Color32::from_rgb(40, 24, 0), c, "?", pal::ORANGE)
        }
        TrackState::Bad => {
            let missing = sectors.map(|(ok, tot)| tot.saturating_sub(ok)).unwrap_or(0);
            let g = if (1..=9).contains(&missing) { DIGITS[missing as usize] } else { "X" };
            (pal::RED_DARK, pal::RED, g, Color32::from_rgb(255, 210, 200))
        }
        TrackState::Empty => {
            (Color32::from_rgb(10, 20, 50), pal::BLUE, "U", Color32::from_rgb(140, 170, 255))
        }
        TrackState::Flux => (Color32::from_rgb(0, 40, 50), pal::CYAN, "F", pal::CYAN),
        TrackState::OutOfRange => (empty_bg, pal::LINE_DIM, "-", pal::GREY),
    }
}

/// Interaction state of a hotspot.
#[derive(Default)]
pub(super) struct Hot {
    hover: bool,
    click: bool,
    rclick: bool,
    down: bool,
    scroll: i32,
}

impl Hot {
    pub(super) fn new(ui: &Ui, live: bool, id: &str, r: Rect) -> Self {
        if !live {
            return Self::default();
        }
        let resp = ui.interact(r, Id::new(("xc", id)), Sense::click());
        let hover = resp.hovered();
        if hover {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let scroll = if hover {
            let dy: f32 = ui.ctx().input(|i| {
                i.events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => delta.y,
                        _ => 0.0,
                    })
                    .sum()
            });
            if dy > 0.5 {
                1
            } else if dy < -0.5 {
                -1
            } else {
                0
            }
        } else {
            0
        };
        Self {
            hover,
            click: resp.clicked(),
            rclick: resp.secondary_clicked(),
            down: resp.is_pointer_button_down_on(),
            scroll,
        }
    }
}
