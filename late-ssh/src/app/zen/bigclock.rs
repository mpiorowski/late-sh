//! Block digits for the clock tile: a 3x5 cell font, doubled when the tile
//! has the room, so the time reads from across the desk.

use ratatui::text::Line;

const GLYPH_ROWS: usize = 5;
const GLYPH_WIDTH: usize = 3;

fn glyph(ch: char) -> [&'static str; GLYPH_ROWS] {
    match ch {
        '0' => ["███", "█ █", "█ █", "█ █", "███"],
        '1' => [" █ ", "██ ", " █ ", " █ ", "███"],
        '2' => ["███", "  █", "███", "█  ", "███"],
        '3' => ["███", "  █", "███", "  █", "███"],
        '4' => ["█ █", "█ █", "███", "  █", "  █"],
        '5' => ["███", "█  ", "███", "  █", "███"],
        '6' => ["███", "█  ", "███", "█ █", "███"],
        '7' => ["███", "  █", "  █", "  █", "  █"],
        '8' => ["███", "█ █", "███", "█ █", "███"],
        '9' => ["███", "█ █", "███", "  █", "███"],
        ':' => ["   ", " █ ", "   ", " █ ", "   "],
        _ => ["   ", "   ", "   ", "   ", "   "],
    }
}

/// Width in cells of `text` rendered at `scale`.
pub fn width_for(text: &str, scale: usize) -> usize {
    let glyphs = text.chars().count();
    if glyphs == 0 {
        return 0;
    }
    (glyphs * GLYPH_WIDTH + (glyphs - 1)) * scale
}

pub fn height_for(scale: usize) -> usize {
    GLYPH_ROWS * scale
}

/// The largest scale (1 or 2) that fits `text` into `width` x `height`,
/// or none when even the small font does not fit.
pub fn fitting_scale(text: &str, width: usize, height: usize) -> Option<usize> {
    [2usize, 1]
        .into_iter()
        .find(|scale| width_for(text, *scale) <= width && height_for(*scale) <= height)
}

/// `text` as rows of block cells. Only digits and `:` have shapes; anything
/// else renders blank.
pub fn render(text: &str, scale: usize) -> Vec<Line<'static>> {
    let glyphs: Vec<[&str; GLYPH_ROWS]> = text.chars().map(glyph).collect();
    let mut rows: Vec<String> = Vec::with_capacity(GLYPH_ROWS * scale);
    for row in 0..GLYPH_ROWS {
        let mut line = String::new();
        for (idx, g) in glyphs.iter().enumerate() {
            if idx > 0 {
                line.push_str(&" ".repeat(scale));
            }
            for cell in g[row].chars() {
                for _ in 0..scale {
                    line.push(cell);
                }
            }
        }
        for _ in 0..scale {
            rows.push(line.clone());
        }
    }
    rows.into_iter().map(Line::from).collect()
}
