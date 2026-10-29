// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Drawing primitives of the retro interface. Everything is laid out on a virtual
//! 1280 × 720 screen ([`Canvas`]) that is scaled to the window, like an Amiga screen on a
//! modern monitor.

use eframe::egui::{
    self, Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Vec2,
};

use crate::ui::font::{Font, Style};

pub const W: f32 = 1280.0;
pub const H: f32 = 720.0;

pub mod pal {
    use eframe::egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(6, 8, 18);
    pub const PANEL: Color32 = Color32::from_rgb(9, 13, 28);
    pub const LINE: Color32 = Color32::from_rgb(78, 158, 186);
    pub const LINE_DIM: Color32 = Color32::from_rgb(34, 70, 92);
    pub const CYAN: Color32 = Color32::from_rgb(150, 220, 236);
    pub const CYAN_DARK: Color32 = Color32::from_rgb(64, 132, 158);
    pub const YELLOW: Color32 = Color32::from_rgb(255, 222, 40);
    pub const YELLOW_TOP: Color32 = Color32::from_rgb(255, 246, 150);
    pub const YELLOW_BOT: Color32 = Color32::from_rgb(236, 160, 0);
    pub const ORANGE: Color32 = Color32::from_rgb(255, 150, 20);
    pub const GREEN: Color32 = Color32::from_rgb(70, 236, 70);
    pub const GREEN_DARK: Color32 = Color32::from_rgb(10, 70, 16);
    pub const GREEN_BOX: Color32 = Color32::from_rgb(20, 120, 30);
    pub const RED: Color32 = Color32::from_rgb(255, 60, 50);
    pub const RED_DARK: Color32 = Color32::from_rgb(110, 14, 10);
    pub const WHITE: Color32 = Color32::from_rgb(236, 240, 250);
    pub const GREY: Color32 = Color32::from_rgb(120, 128, 140);
    pub const BLUE: Color32 = Color32::from_rgb(40, 90, 220);
}

/// Virtual-screen → window mapping.
#[derive(Clone, Copy)]
pub struct Canvas {
    pub o: Pos2,
    pub s: f32,
}

impl Canvas {
    pub fn fit(screen: Rect) -> Self {
        let s = (screen.width() / W).min(screen.height() / H);
        let o = screen.center() - Vec2::new(W * s, H * s) / 2.0;
        Self { o, s }
    }
    pub fn p(&self, x: f32, y: f32) -> Pos2 {
        self.o + Vec2::new(x, y) * self.s
    }
    pub fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.p(x, y), Vec2::new(w, h) * self.s)
    }
    pub fn l(&self, v: f32) -> f32 {
        v * self.s
    }
    /// Window position → virtual coordinates.
    pub fn inv(&self, p: Pos2) -> Pos2 {
        ((p - self.o) / self.s).to_pos2()
    }
}

/// Look of a push button.
#[derive(Clone, Copy)]
pub struct Button<'a> {
    pub label: &'a str,
    pub color: Color32,
    pub hover: bool,
    pub pressed: bool,
}

/// Painter + canvas + font, passed around by the UI code.
pub struct Gfx<'a> {
    pub p: Painter,
    pub c: Canvas,
    pub font: &'a Font,
    pub dots: &'a TextureHandle,
    pub t: f64,
}

pub fn dots_texture(ctx: &egui::Context) -> TextureHandle {
    // 6 × 6 tile with one bevelled dot: the perforated metal of the title plate.
    let n = 6;
    let mut px = vec![Color32::from_rgb(8, 12, 26); n * n];
    px[n + 1] = Color32::from_rgb(54, 84, 104);
    px[n + 2] = Color32::from_rgb(34, 56, 74);
    px[2 * n + 1] = Color32::from_rgb(34, 56, 74);
    px[2 * n + 2] = Color32::from_rgb(18, 30, 44);
    let img = egui::ColorImage::new([n, n], px);
    ctx.load_texture("dots", img, TextureOptions::NEAREST_REPEAT)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl Gfx<'_> {
    pub fn text(&self, x: f32, y: f32, size: f32, text: &str, style: Style) {
        let px = self.c.l(size);
        self.font.draw(&self.p, self.c.p(x, y), Vec2::splat(px), text, style);
    }

    /// Text with explicit horizontal/vertical pixel sizes (stretched lettering).
    pub fn text2(&self, x: f32, y: f32, sx: f32, sy: f32, text: &str, style: Style) {
        self.font.draw(&self.p, self.c.p(x, y), Vec2::new(self.c.l(sx), self.c.l(sy)), text, style);
    }

    pub fn text_centered(&self, cx: f32, y: f32, size: f32, text: &str, style: Style) {
        let w = Font::width(text, size);
        self.text(cx - w / 2.0, y, size, text, style);
    }

    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.p.rect_filled(self.c.r(x, y, w, h), CornerRadius::ZERO, c);
    }

    pub fn stroke(&self, x: f32, y: f32, w: f32, h: f32, width: f32, c: Color32) {
        self.p.rect_stroke(
            self.c.r(x, y, w, h),
            CornerRadius::ZERO,
            Stroke::new(self.c.l(width), c),
            StrokeKind::Inside,
        );
    }

    /// Vertical gradient rectangle.
    pub fn vgrad(&self, x: f32, y: f32, w: f32, h: f32, top: Color32, bottom: Color32) {
        let mut m = Mesh::default();
        let r = self.c.r(x, y, w, h);
        let i = m.vertices.len() as u32;
        m.colored_vertex(r.left_top(), top);
        m.colored_vertex(r.right_top(), top);
        m.colored_vertex(r.left_bottom(), bottom);
        m.colored_vertex(r.right_bottom(), bottom);
        m.add_triangle(i, i + 1, i + 2);
        m.add_triangle(i + 1, i + 3, i + 2);
        self.p.add(Shape::mesh(m));
    }

    /// Perforated-metal background.
    pub fn dotted(&self, x: f32, y: f32, w: f32, h: f32) {
        let r = self.c.r(x, y, w, h);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(w / 6.0, h / 6.0));
        let mut m = Mesh::with_texture(self.dots.id());
        m.add_rect_with_uv(r, uv, Color32::WHITE);
        self.p.add(Shape::mesh(m));
    }

    /// One metal bead (rivet) of a frame.
    pub fn bead(&self, x: f32, y: f32, r: f32) {
        let c = self.c.p(x, y);
        let r = self.c.l(r);
        self.p.circle_filled(c + Vec2::splat(r * 0.25), r, Color32::from_rgb(4, 10, 16));
        self.p.circle_filled(c, r, Color32::from_rgb(52, 110, 132));
        self.p.circle_filled(c - Vec2::splat(r * 0.18), r * 0.72, Color32::from_rgb(96, 168, 190));
        self.p.circle_filled(c - Vec2::splat(r * 0.36), r * 0.32, Color32::from_rgb(214, 246, 252));
    }

    /// Panel framed by a chain of beads.
    pub fn bead_frame(&self, x: f32, y: f32, w: f32, h: f32) {
        self.fill(x, y, w, h, Color32::from_rgb(16, 30, 44));
        self.fill(x + 16.0, y + 16.0, w - 32.0, h - 32.0, pal::PANEL);
        let step = 15.5;
        let r = 6.4;
        let nx = ((w - 16.0) / step).round().max(1.0);
        let ny = ((h - 16.0) / step).round().max(1.0);
        let sx = (w - 16.0) / nx;
        let sy = (h - 16.0) / ny;
        for i in 0..=nx as i32 {
            let bx = x + 8.0 + i as f32 * sx;
            self.bead(bx, y + 8.0, r);
            self.bead(bx, y + h - 8.0, r);
        }
        for j in 1..ny as i32 {
            let by = y + 8.0 + j as f32 * sy;
            self.bead(x + 8.0, by, r);
            self.bead(x + w - 8.0, by, r);
        }
        self.stroke(x + 15.0, y + 15.0, w - 30.0, h - 30.0, 1.5, pal::LINE);
    }

    /// Double-line technical frame (option bar, drive and track panels).
    pub fn line_frame(&self, x: f32, y: f32, w: f32, h: f32) {
        self.fill(x, y, w, h, pal::PANEL);
        self.stroke(x, y, w, h, 2.0, pal::LINE);
        self.stroke(x + 4.0, y + 4.0, w - 8.0, h - 8.0, 1.0, pal::LINE_DIM);
    }

    /// Bevelled push button. `pressed` swaps the light and dark edges.
    pub fn button(&self, x: f32, y: f32, w: f32, h: f32, b: &Button) {
        let Button { label, color, hover, pressed } = *b;
        let (hi, lo) = if pressed {
            (Color32::from_rgb(10, 16, 24), Color32::from_rgb(150, 190, 210))
        } else {
            (Color32::from_rgb(150, 190, 210), Color32::from_rgb(10, 16, 24))
        };
        self.fill(x, y, w, h, Color32::from_rgb(36, 54, 70));
        self.fill(x, y, w, 3.0, hi);
        self.fill(x, y, 3.0, h, hi);
        self.fill(x, y + h - 3.0, w, 3.0, lo);
        self.fill(x + w - 3.0, y, 3.0, h, lo);
        let fill = if hover { Color32::from_rgb(16, 30, 46) } else { Color32::from_rgb(8, 14, 26) };
        self.fill(x + 6.0, y + 6.0, w - 12.0, h - 12.0, fill);
        let size = 2.25;
        let off = if pressed { 1.5 } else { 0.0 };
        let ty = y + (h - 8.0 * size) / 2.0 + off;
        let style = Style::grad(lerp(color, Color32::WHITE, 0.45), color);
        self.text_centered(x + w / 2.0 + off, ty, size, label, style);
    }

    /// Light bulb: lit (selected drive) or cold glass.
    pub fn bulb(&self, cx: f32, cy: f32, on: bool, enabled: bool) {
        let c = self.c.p(cx, cy);
        let r = self.c.l(14.0);
        if on {
            for (i, a) in [(2.4, 22u8), (1.8, 40), (1.35, 70)] {
                self.p.circle_filled(c, r * i, Color32::from_rgba_unmultiplied(255, 200, 30, a));
            }
        }
        // screw base
        let base = self.c.r(cx - 7.0, cy + 11.0, 14.0, 12.0);
        self.p.rect_filled(base, CornerRadius::same(2), Color32::from_rgb(120, 130, 140));
        for k in 0..3 {
            let yy = cy + 13.0 + k as f32 * 3.5;
            self.p.line_segment(
                [self.c.p(cx - 7.0, yy), self.c.p(cx + 7.0, yy)],
                Stroke::new(self.c.l(1.2), Color32::from_rgb(60, 66, 74)),
            );
        }
        self.p.circle_filled(self.c.p(cx, cy + 25.0), self.c.l(3.0), Color32::from_rgb(70, 76, 84));
        let (glass, mid, hi) = match (on, enabled) {
            (true, _) => (
                Color32::from_rgb(232, 150, 0),
                Color32::from_rgb(255, 210, 40),
                Color32::from_rgb(255, 250, 200),
            ),
            (false, true) => (
                Color32::from_rgb(70, 150, 180),
                Color32::from_rgb(130, 210, 230),
                Color32::from_rgb(230, 250, 255),
            ),
            (false, false) => (
                Color32::from_rgb(40, 50, 60),
                Color32::from_rgb(60, 72, 84),
                Color32::from_rgb(96, 108, 120),
            ),
        };
        self.p.circle_filled(c, r, glass);
        self.p.circle_filled(c - Vec2::new(r * 0.12, r * 0.15), r * 0.78, mid);
        self.p.circle_filled(c - Vec2::new(r * 0.38, r * 0.4), r * 0.28, hi);
        if on {
            // filament
            self.p.line_segment(
                [self.c.p(cx - 4.0, cy + 3.0), self.c.p(cx + 4.0, cy + 3.0)],
                Stroke::new(self.c.l(1.6), Color32::from_rgb(255, 255, 255)),
            );
        }
    }

    /// 3.5" floppy drawn as the drive icon. `led`: activity light of the drive.
    pub fn floppy(&self, x: f32, y: f32, n: usize, enabled: bool, led: Option<Color32>, hover: bool) {
        let (w, h) = (104.0, 96.0);
        let body = if enabled { Color32::from_rgb(30, 84, 210) } else { Color32::from_rgb(40, 46, 60) };
        let body_dark = if enabled { Color32::from_rgb(16, 44, 130) } else { Color32::from_rgb(26, 30, 40) };
        self.fill(x + 3.0, y + 3.0, w, h, Color32::from_rgb(0, 0, 0));
        self.vgrad(x, y, w, h, body, body_dark);
        self.stroke(
            x,
            y,
            w,
            h,
            2.0,
            if hover && enabled { pal::CYAN } else { Color32::from_rgb(10, 20, 60) },
        );
        // label
        let label_bg =
            if enabled { Color32::from_rgb(244, 244, 236) } else { Color32::from_rgb(120, 124, 130) };
        self.fill(x + 12.0, y + 2.0, w - 24.0, 52.0, label_bg);
        self.fill(x + 12.0, y + 2.0, w - 24.0, 7.0, if enabled { pal::YELLOW } else { pal::GREY });
        let ink = Style::flat(Color32::from_rgb(20, 30, 60));
        self.text_centered(x + w / 2.0, y + 15.0, 2.0, "DRIVE", ink);
        self.text_centered(x + w / 2.0, y + 34.0, 2.0, &n.to_string(), ink);
        // metal shutter
        let shutter = if enabled { Color32::from_rgb(196, 206, 216) } else { Color32::from_rgb(90, 96, 104) };
        self.vgrad(x + 26.0, y + 62.0, 52.0, 34.0, shutter, lerp(shutter, Color32::BLACK, 0.35));
        self.fill(x + 34.0, y + 68.0, 12.0, 22.0, body_dark);
        // write-protect notch and activity LED
        self.fill(x + 6.0, y + 82.0, 9.0, 9.0, Color32::from_rgb(8, 12, 30));
        if let Some(c) = led {
            let p = self.c.p(x + w - 11.0, y + 86.0);
            self.p.circle_filled(p, self.c.l(9.0), Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 60));
            self.p.circle_filled(p, self.c.l(4.5), c);
        } else {
            self.p.circle_filled(
                self.c.p(x + w - 11.0, y + 86.0),
                self.c.l(4.0),
                Color32::from_rgb(40, 10, 10),
            );
        }
    }

    /// Small up/down arrow button.
    pub fn arrow(&self, cx: f32, cy: f32, up: bool, hover: bool) {
        let (w, h) = (9.0, 8.0);
        let pts = if up {
            vec![self.c.p(cx, cy - h / 2.0), self.c.p(cx + w, cy + h / 2.0), self.c.p(cx - w, cy + h / 2.0)]
        } else {
            vec![self.c.p(cx - w, cy - h / 2.0), self.c.p(cx + w, cy - h / 2.0), self.c.p(cx, cy + h / 2.0)]
        };
        let col = if hover { pal::WHITE } else { pal::CYAN };
        self.p.add(Shape::convex_polygon(pts, col, Stroke::new(self.c.l(1.0), pal::CYAN_DARK)));
    }

    /// Orange "clickable" dot in front of option headers.
    pub fn knob(&self, cx: f32, cy: f32, hover: bool) {
        let c = self.c.p(cx, cy);
        let r = self.c.l(6.0);
        self.p.circle_filled(c, r, if hover { pal::YELLOW } else { pal::ORANGE });
        self.p.circle_filled(c - Vec2::splat(r * 0.3), r * 0.4, Color32::from_rgb(255, 240, 180));
    }
}

/// The title logo: big lettering made of bevelled pixel bricks with a moving shine.
pub fn logo(g: &Gfx, x: f32, y: f32, text: &str, brick: f32) {
    let shine = ((g.t * 0.35) % 1.6) as f32 - 0.3;
    let total = text.chars().count() as f32 * 9.0 * brick;
    for (i, ch) in text.chars().enumerate() {
        let gl = crate::ui::font::glyph(ch);
        for (row, bits) in gl.iter().enumerate() {
            // emboldened: every pixel also lights its right neighbour
            let bits = *bits as u16 | ((*bits as u16) << 1);
            for col in 0..9 {
                if bits & (1 << col) == 0 {
                    continue;
                }
                let bx = x + (i as f32 * 9.0 + col as f32) * brick;
                let by = y + row as f32 * brick;
                let t = row as f32 / 7.0;
                let rel = (bx - x) / total;
                let glow = (1.0 - ((rel - shine).abs() * 6.0)).clamp(0.0, 1.0);
                let base = lerp(Color32::from_rgb(200, 236, 246), Color32::from_rgb(70, 132, 160), t);
                let base = lerp(base, Color32::WHITE, glow * 0.7);
                let b = brick - 1.0;
                g.fill(bx + 3.0, by + 3.0, b, b, Color32::from_rgb(0, 4, 10));
                g.fill(bx, by, b, b, base);
                g.fill(bx, by, b, 2.5, lerp(base, Color32::WHITE, 0.6));
                g.fill(bx, by, 2.5, b, lerp(base, Color32::WHITE, 0.4));
                g.fill(bx, by + b - 2.5, b, 2.5, lerp(base, Color32::BLACK, 0.5));
                g.fill(bx + b - 2.5, by, 2.5, b, lerp(base, Color32::BLACK, 0.4));
            }
        }
    }
}
