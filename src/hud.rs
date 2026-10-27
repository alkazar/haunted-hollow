//! 2D overlay builder: rectangles, sprites and blocky pixel-font text.

use crate::atlas::{glyph_uv, tile_uv, T_WHITE};
use crate::vk::Vertex;

pub struct Ui {
    pub v: Vec<Vertex>,
    pub w: f32,
    pub h: f32,
    /// One "pixel" of the pixel font at scale 1, in screen pixels.
    pub unit: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Ui {
    pub fn new(w: f32, h: f32) -> Ui {
        Ui { v: Vec::with_capacity(8192), w, h, unit: (h / 240.0).max(1.0) }
    }

    pub fn quad_uv(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], c: [f32; 4]) {
        let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        let t = [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]];
        for i in 0..4 {
            self.v.push(Vertex { pos: [p[i][0], p[i][1], 0.0], uv: t[i], color: c });
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        self.quad_uv(x, y, w, h, tile_uv(T_WHITE), c);
    }

    pub fn sprite(&mut self, tile: u8, cx: f32, cy: f32, size: f32, c: [f32; 4]) {
        self.quad_uv(cx - size * 0.5, cy - size * 0.5, size, size, tile_uv(tile), c);
    }

    pub fn text_width(&self, s: &str, scale: f32) -> f32 {
        s.len() as f32 * 8.0 * scale * self.unit
    }

    /// Draws text with a drop shadow. `scale` is in font pixels per unit.
    pub fn text(&mut self, s: &str, x: f32, y: f32, scale: f32, c: [f32; 4], align: Align) {
        let w = self.text_width(s, scale);
        let x = match align {
            Align::Left => x,
            Align::Center => x - w * 0.5,
            Align::Right => x - w,
        };
        let px = scale * self.unit;
        let shadow = [0.0, 0.0, 0.0, c[3] * 0.8];
        for pass in 0..2 {
            let (ox, oy, col) = if pass == 0 { (px, px, shadow) } else { (0.0, 0.0, c) };
            for (i, ch) in s.bytes().enumerate() {
                if ch == b' ' {
                    continue;
                }
                let cx = x + i as f32 * 8.0 * px + ox;
                self.quad_uv(cx, y + oy, 8.0 * px, 8.0 * px, glyph_uv(ch.min(127)), col);
            }
        }
    }

    /// Like `text` but shrinks the scale so the line fits within `max_w` pixels.
    pub fn text_fit(&mut self, s: &str, x: f32, y: f32, scale: f32, max_w: f32, c: [f32; 4], align: Align) {
        let w = self.text_width(s, scale);
        let scale = if w > max_w { scale * max_w / w } else { scale };
        self.text(s, x, y, scale, c, align);
    }

    /// Rectangle with stepped (pixel-art) rounded corners of radius `r`.
    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: [f32; 4]) {
        let r = r.min(w * 0.5).min(h * 0.5);
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, w, h - 2.0 * r, c);
        let q = r * 0.35;
        self.rect(x + q, y + q, w - 2.0 * q, h - 2.0 * q, c);
    }

    /// Frame of a panel with a darker fill.
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32, fill: [f32; 4], border: [f32; 4]) {
        let b = self.unit;
        self.rect(x, y, w, h, border);
        self.rect(x + b, y + b, w - 2.0 * b, h - 2.0 * b, fill);
    }
}
