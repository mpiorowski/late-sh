//! A half-block drawing surface.
//!
//! Each terminal cell carries two vertically stacked pixels: `▀` with the
//! foreground painting the top and the background the bottom. So a canvas
//! `cols` wide and `rows` tall is a `cols x 2*rows` pixel buffer, and since a
//! terminal cell is roughly twice as tall as it is wide, those pixels come out
//! very nearly square. That is the whole reason to use half blocks here rather
//! than plain characters: circles look like circles.
//!
//! Related but not reusable: `app/lobby/house/image_render.rs` does the same
//! `▀` trick, but it converts a finished `RgbaImage`. This is a surface you
//! draw *into*, and it lives in `games` because `games` may not depend on
//! `lobby`.
//!
//! Glyph overrides sit on top. A cell carrying a glyph draws that character
//! instead of the block, which is how a ball wears its number when there is
//! room for one.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Glyph {
    ch: char,
    fg: Rgb,
}

pub struct Canvas {
    cols: u16,
    /// Pixel rows: always `2 * terminal rows`.
    height: u16,
    pixels: Vec<Rgb>,
    /// One slot per terminal cell, not per pixel.
    glyphs: Vec<Option<Glyph>>,
    background: Rgb,
}

impl Canvas {
    pub fn new(cols: u16, rows: u16, background: Rgb) -> Self {
        let height = rows.saturating_mul(2);
        Self {
            cols,
            height,
            pixels: vec![background; cols as usize * height as usize],
            glyphs: vec![None; cols as usize * rows as usize],
            background,
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Height in pixels, which is twice the height in terminal rows.
    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn rows(&self) -> u16 {
        self.height / 2
    }

    pub fn set(&mut self, x: i32, y: i32, colour: Rgb) {
        if x < 0 || y < 0 || x >= self.cols as i32 || y >= self.height as i32 {
            return;
        }
        let index = y as usize * self.cols as usize + x as usize;
        self.pixels[index] = colour;
    }

    pub fn get(&self, x: i32, y: i32) -> Rgb {
        if x < 0 || y < 0 || x >= self.cols as i32 || y >= self.height as i32 {
            return self.background;
        }
        self.pixels[y as usize * self.cols as usize + x as usize]
    }

    /// Put a character in the terminal cell containing pixel row `y`.
    pub fn glyph(&mut self, x: i32, y: i32, ch: char, fg: Rgb) {
        let row = y / 2;
        if x < 0 || row < 0 || x >= self.cols as i32 || row >= self.rows() as i32 {
            return;
        }
        let index = row as usize * self.cols as usize + x as usize;
        self.glyphs[index] = Some(Glyph { ch, fg });
    }

    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, colour: Rgb) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, colour);
            }
        }
    }

    /// A filled circle, centre and radius in pixels.
    ///
    /// Sampled at pixel centres rather than antialiased: at three or four
    /// pixels across, blending edges just makes a ball look like a smudge.
    pub fn disc(&mut self, cx: f64, cy: f64, radius: f64, colour: Rgb) {
        let r2 = radius * radius;
        let min_x = (cx - radius).floor() as i32;
        let max_x = (cx + radius).ceil() as i32;
        let min_y = (cy - radius).floor() as i32;
        let max_y = (cy + radius).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    self.set(x, y, colour);
                }
            }
        }
    }

    /// A dotted straight line, used for aim guides. `on`/`off` are in pixels;
    /// pass `off = 0` for a solid line.
    pub fn line(&mut self, from: (f64, f64), to: (f64, f64), colour: Rgb, on: u32, off: u32) {
        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let steps = dx.abs().max(dy.abs()).ceil() as i32;
        if steps <= 0 {
            return;
        }
        let period = (on + off).max(1);
        for step in 0..=steps {
            if off > 0 && (step as u32 % period) >= on {
                continue;
            }
            let t = step as f64 / steps as f64;
            self.set(
                (from.0 + dx * t).round() as i32,
                (from.1 + dy * t).round() as i32,
                colour,
            );
        }
    }

    /// Collapse to terminal lines. Each cell is `▀` with the top pixel as the
    /// foreground and the bottom as the background, unless a glyph overrides
    /// it — in which case the glyph keeps the bottom pixel as its background,
    /// so a numbered ball still reads as that ball's colour.
    pub fn to_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(self.rows() as usize);
        for row in 0..self.rows() {
            let mut spans = Vec::with_capacity(self.cols as usize);
            for col in 0..self.cols {
                let x = col as i32;
                let top = self.get(x, row as i32 * 2);
                let bottom = self.get(x, row as i32 * 2 + 1);
                let glyph = self.glyphs[row as usize * self.cols as usize + col as usize];
                spans.push(match glyph {
                    Some(Glyph { ch, fg }) => {
                        Span::styled(ch.to_string(), Style::default().fg(rgb(fg)).bg(rgb(bottom)))
                    }
                    None => Span::styled("▀", Style::default().fg(rgb(top)).bg(rgb(bottom))),
                });
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

pub fn rgb(c: Rgb) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

/// Blend `a` toward `b` by `t` in 0..=1. Used for shading and highlights.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t).round() as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t).round() as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t).round() as u8,
    ]
}
