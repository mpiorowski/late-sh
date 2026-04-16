use qrcodegen::QrCode;
use ratatui::text::{Line, Span};

use super::polarity::Polarity;

pub trait Barcode {
    /// QR modules consumed per glyph horizontally.
    const MODULES_W: i32;
    /// QR modules consumed per glyph vertically.
    const MODULES_H: i32;
    /// Terminal chars emitted per glyph (FullBlock uses 2 for square aspect).
    const CHARS_PER_GLYPH: usize;

    /// Modules packed as bits `(dy * MODULES_W + dx)`, set if module is ON.
    fn glyph(modules: u32) -> char;
}

pub struct HalfBlock;

impl Barcode for HalfBlock {
    const MODULES_W: i32 = 1;
    const MODULES_H: i32 = 2;
    const CHARS_PER_GLYPH: usize = 1;

    fn glyph(modules: u32) -> char {
        let top = (modules & 0b01) != 0;
        let bot = (modules & 0b10) != 0;
        match (top, bot) {
            (false, false) => ' ',
            (true, false) => '\u{2580}', // ▀
            (false, true) => '\u{2584}', // ▄
            (true, true) => '\u{2588}',  // █
        }
    }
}

pub struct Braille;

impl Barcode for Braille {
    const MODULES_W: i32 = 2;
    const MODULES_H: i32 = 4;
    const CHARS_PER_GLYPH: usize = 1;

    fn glyph(modules: u32) -> char {
        // covers range [U+2800, U+28FF]
        const DOT_BITS: [u32; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
        let mut mask = 0u32;
        for (i, &bit) in DOT_BITS.iter().enumerate() {
            if (modules >> i) & 1 != 0 {
                mask |= bit;
            }
        }
        char::from_u32(0x2800 + mask).unwrap_or(' ')
    }
}

pub struct FullBlock;

impl Barcode for FullBlock {
    const MODULES_W: i32 = 1;
    const MODULES_H: i32 = 1;
    const CHARS_PER_GLYPH: usize = 2;

    fn glyph(modules: u32) -> char {
        if modules & 1 != 0 { '\u{2588}' } else { ' ' }
    }
}

fn push_glyph<B: Barcode>(row: &mut String, glyph: char) {
    for _ in 0..B::CHARS_PER_GLYPH {
        row.push(glyph);
    }
}

pub(super) fn render<'a, B: Barcode, P: Polarity>(qr: &QrCode) -> Vec<Line<'a>> {
    let size = qr.size();
    let style = P::style();
    let quiet_zone: i32 = 4;

    let pad_x_glyphs = (quiet_zone + B::MODULES_W - 1) / B::MODULES_W;
    let pad_y = (quiet_zone + B::MODULES_H - 1) / B::MODULES_H;
    let qr_glyphs_w = (size + B::MODULES_W - 1) / B::MODULES_W;
    let data_rows = (size + B::MODULES_H - 1) / B::MODULES_H;

    // Auto-correct aspect: if visual height > width, add cosmetic horizontal
    // padding so the QR appears square (terminal cells are ~1:2 wide:tall).
    let data_chars_w = qr_glyphs_w as usize * B::CHARS_PER_GLYPH;
    let visual_w = data_chars_w;
    let visual_h = data_rows as usize * 2;
    let extra_pad = if visual_h > visual_w {
        (visual_h - visual_w).div_ceil(2)
    } else {
        0
    };

    let pad_x_chars = pad_x_glyphs as usize * B::CHARS_PER_GLYPH + extra_pad;
    let full_width = pad_x_chars * 2 + data_chars_w;
    let off_glyph = B::glyph(0);
    let pad_row: String = std::iter::repeat_n(off_glyph, full_width).collect();

    let mut lines: Vec<Line<'a>> = Vec::with_capacity(pad_y as usize * 2 + data_rows as usize);

    for _ in 0..pad_y {
        lines.push(Line::from(Span::styled(pad_row.clone(), style)));
    }

    let mut y = 0;
    while y < size {
        let mut row = String::with_capacity(full_width * 3);
        for _ in 0..pad_x_chars {
            row.push(off_glyph);
        }
        let mut x = 0;
        while x < size {
            let mut modules: u32 = 0;
            for dy in 0..B::MODULES_H {
                for dx in 0..B::MODULES_W {
                    let mx = x + dx;
                    let my = y + dy;
                    if mx < size && my < size && qr.get_module(mx, my) {
                        modules |= 1 << (dy * B::MODULES_W + dx);
                    }
                }
            }
            push_glyph::<B>(&mut row, B::glyph(modules));
            x += B::MODULES_W;
        }
        for _ in 0..pad_x_chars {
            row.push(off_glyph);
        }
        lines.push(Line::from(Span::styled(row, style)));
        y += B::MODULES_H;
    }

    for _ in 0..pad_y {
        lines.push(Line::from(Span::styled(pad_row.clone(), style)));
    }

    lines
}
