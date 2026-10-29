// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! Bitmap font (public-domain `font8x8` glyphs) rendered from a texture atlas, nearest-filtered,
//! with a vertical colour gradient and a drop shadow — the chunky 16-bit look.

use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use font8x8::legacy::{BASIC_LEGACY, LATIN_LEGACY};

/// Atlas: 16 × 16 cells of 8 × 8 pixels (code points 0..=255).
const CELLS: usize = 16;

pub struct Font {
    tex: TextureHandle,
}

/// Glyph bitmap of a Latin-1 character (unknown → '?').
pub fn glyph(c: char) -> [u8; 8] {
    let code = c as u32;
    match code {
        0..=127 => BASIC_LEGACY[code as usize],
        0xA0..=0xFF => LATIN_LEGACY[(code - 0xA0) as usize],
        _ => BASIC_LEGACY[b'?' as usize],
    }
}

impl Font {
    pub fn new(ctx: &egui::Context) -> Self {
        let size = CELLS * 8;
        let mut px = vec![Color32::TRANSPARENT; size * size];
        for code in 0..256u32 {
            let Some(c) = char::from_u32(code) else { continue };
            let g = glyph(c);
            let (cx, cy) = ((code as usize % CELLS) * 8, (code as usize / CELLS) * 8);
            for (y, row) in g.iter().enumerate() {
                for x in 0..8 {
                    if row & (1 << x) != 0 {
                        px[(cy + y) * size + cx + x] = Color32::WHITE;
                    }
                }
            }
        }
        let img = egui::ColorImage::new([size, size], px);
        let tex = ctx.load_texture("pixel-font", img, TextureOptions::NEAREST);
        Self { tex }
    }

    fn uv(code: u32) -> Rect {
        let code = if code > 255 { b'?' as u32 } else { code };
        let s = 1.0 / CELLS as f32;
        let (x, y) = ((code as usize % CELLS) as f32 * s, (code as usize / CELLS) as f32 * s);
        Rect::from_min_size(Pos2::new(x, y), Vec2::splat(s))
    }

    fn push_glyphs(&self, mesh: &mut Mesh, pos: Pos2, px: Vec2, text: &str, top: Color32, bottom: Color32) {
        let mut x = pos.x;
        for ch in text.chars() {
            if ch != ' ' {
                let r = Rect::from_min_size(Pos2::new(x, pos.y), Vec2::new(8.0 * px.x, 8.0 * px.y));
                let uv = Self::uv(ch as u32);
                let i = mesh.vertices.len() as u32;
                let v = |p: Pos2, u: Pos2, c: Color32| egui::epaint::Vertex { pos: p, uv: u, color: c };
                mesh.vertices.push(v(r.left_top(), uv.left_top(), top));
                mesh.vertices.push(v(r.right_top(), uv.right_top(), top));
                mesh.vertices.push(v(r.left_bottom(), uv.left_bottom(), bottom));
                mesh.vertices.push(v(r.right_bottom(), uv.right_bottom(), bottom));
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i + 1, i + 3, i + 2);
            }
            x += 8.0 * px.x;
        }
    }

    /// Draw `text` with its top-left at `pos`; `px` is the size of one font pixel.
    pub fn draw(&self, p: &Painter, pos: Pos2, px: Vec2, text: &str, style: Style) {
        let mut mesh = Mesh::with_texture(self.tex.id());
        if let Some(sh) = style.shadow {
            let off = Vec2::new(px.x.max(1.0), px.y.max(1.0));
            self.push_glyphs(&mut mesh, pos + off, px, text, sh, sh);
        }
        self.push_glyphs(&mut mesh, pos, px, text, style.top, style.bottom);
        p.add(egui::Shape::mesh(mesh));
    }

    pub fn width(text: &str, px: f32) -> f32 {
        text.chars().count() as f32 * 8.0 * px
    }
}

#[derive(Clone, Copy)]
pub struct Style {
    pub top: Color32,
    pub bottom: Color32,
    pub shadow: Option<Color32>,
}

impl Style {
    pub const fn flat(c: Color32) -> Self {
        Self { top: c, bottom: c, shadow: None }
    }
    pub const fn grad(top: Color32, bottom: Color32) -> Self {
        Self { top, bottom, shadow: Some(Color32::from_rgb(0, 0, 0)) }
    }
    pub const fn shadowed(c: Color32) -> Self {
        Self { top: c, bottom: c, shadow: Some(Color32::from_rgb(0, 0, 0)) }
    }
}

/// Word-wrap for a fixed-width font.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let mut word = word.to_string();
            while word.chars().count() > max {
                let head: String = word.chars().take(max).collect();
                word = word.chars().skip(max).collect();
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                out.push(head);
            }
            if line.is_empty() {
                line = word;
            } else if line.chars().count() + 1 + word.chars().count() <= max {
                line.push(' ');
                line.push_str(&word);
            } else {
                out.push(std::mem::replace(&mut line, word));
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(
            wrap("insert disk in DF0 and press ok", 12),
            vec!["insert disk", "in DF0 and", "press ok"]
        );
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }
}
