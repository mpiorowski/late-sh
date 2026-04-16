pub trait Barcode {
    /// QR modules consumed per glyph horizontally.
    const MODULES_W: i32;
    /// QR modules consumed per glyph vertically.
    const MODULES_H: i32;

    /// Modules packed as bits `(dy * MODULES_W + dx)`, set if module is ON.
    fn glyph(modules: u32) -> char;
}

pub struct HalfBlock;

impl Barcode for HalfBlock {
    const MODULES_W: i32 = 1;
    const MODULES_H: i32 = 2;

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

    fn glyph(modules: u32) -> char {
        if modules & 1 != 0 { '\u{2588}' } else { ' ' }
    }
}
