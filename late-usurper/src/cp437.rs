//! CP437 -> UTF-8 transcoding for the game's terminal output.
//!
//! Usurper is a DOS-era door: its screens (box-drawing art, block shades, the
//! odd accented letter) are byte-oriented CP437, which is mojibake to the
//! UTF-8 `vt100` parser on the late-ssh side. The host transcodes the output
//! stream before it enters the SSH channel so the client stays byte-compatible
//! with the other doors.
//!
//! Only bytes 0x80..=0xFF are mapped. Everything below 0x80 is ASCII in CP437
//! and passes through untouched, which is also what makes this safe to apply
//! to a raw terminal stream: ANSI escape sequences are pure ASCII, so the
//! transcoder never rewrites a byte inside one. (CP437 technically assigns
//! glyphs to 0x01..=0x1F too, but the game uses that range as control
//! characters (ESC, CR, LF, BEL), exactly like the terminal does.)

/// Unicode codepoints for CP437 bytes 0x80..=0xFF, in order.
const HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', // 0x80
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', // 0x90
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', // 0xA0
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', // 0xB0
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', // 0xC0
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', // 0xD0
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', // 0xE0
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■',
    '\u{a0}', // 0xF0
];

/// Transcode a chunk of raw CP437 terminal output into UTF-8 bytes.
pub fn to_utf8(input: &[u8]) -> Vec<u8> {
    // High bytes expand to up to 3 UTF-8 bytes; most chunks are mostly ASCII.
    let mut out = Vec::with_capacity(input.len() + input.len() / 4);
    let mut buf = [0u8; 4];
    for &b in input {
        if b < 0x80 {
            out.push(b);
        } else {
            out.extend_from_slice(HIGH[(b - 0x80) as usize].encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}
