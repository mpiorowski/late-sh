//! A card of text as a cell canvas: the pictures late.sh draws from its
//! own words (the week's scoreboard of every game, the paper's front
//! page), rendered by `board_png` like any board, so they look like the
//! rest of what the server posts. Lines are wrapped here by display width; a card
//! never runs wider than `CARD_COLUMNS` plus its margin.

use dartboard_core::{Canvas, Pos, RgbColor};

/// The text's width in cells. `board_png` scales a 68-cell canvas to
/// about 1500 pixels, so a line of this is a comfortable read.
pub(crate) const CARD_COLUMNS: usize = 64;

/// Blank cells around the text on every side.
const MARGIN: usize = 2;

/// What a line is, which decides its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ink {
    /// The masthead: `text-green-400`.
    Title,
    /// A section's name: `text-green-500`.
    Heading,
    /// The words: the board's default ink.
    Body,
    /// Datelines and footers: the shell's dim text, `#9ca3af`.
    Dim,
}

impl Ink {
    fn colour(self) -> Option<RgbColor> {
        match self {
            Self::Title => Some(RgbColor {
                r: 0x4a,
                g: 0xde,
                b: 0x80,
            }),
            Self::Heading => Some(RgbColor {
                r: 0x22,
                g: 0xc5,
                b: 0x5e,
            }),
            Self::Body => None,
            Self::Dim => Some(RgbColor {
                r: 0x9c,
                g: 0xa3,
                b: 0xaf,
            }),
        }
    }
}

/// One line of a card, at most `CARD_COLUMNS` wide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardLine {
    pub(crate) ink: Ink,
    pub(crate) text: String,
}

impl CardLine {
    pub(crate) fn new(ink: Ink, text: impl Into<String>) -> Self {
        Self {
            ink,
            text: text.into(),
        }
    }

    pub(crate) fn blank() -> Self {
        Self::new(Ink::Body, "")
    }
}

/// `text` word-wrapped to `CARD_COLUMNS`, at most `max_lines` of it; a
/// cut paragraph ends in an ellipsis. A word wider than a line is broken.
pub(crate) fn paragraph(ink: Ink, text: &str, max_lines: usize) -> Vec<CardLine> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        loop {
            let room = match current.is_empty() {
                true => CARD_COLUMNS,
                false => CARD_COLUMNS.saturating_sub(width(&current) + 1),
            };
            if width(&word) <= room {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&word);
                break;
            }
            if current.is_empty() {
                let (head, tail) = split_at_width(&word, CARD_COLUMNS);
                lines.push(head);
                word = tail;
            } else {
                lines.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            // Room for the ellipsis: drop the last word, or the last
            // characters of a line that is one word.
            while width(last) + 1 > CARD_COLUMNS {
                match last.rfind(' ') {
                    Some(at) => last.truncate(at),
                    None => *last = split_at_width(last, CARD_COLUMNS - 1).0,
                }
            }
            last.push('…');
        }
    }
    lines
        .into_iter()
        .map(|text| CardLine::new(ink, text))
        .collect()
}

/// The card as a canvas: the lines top to bottom inside the margin.
pub(crate) fn canvas(lines: &[CardLine]) -> Canvas {
    let mut canvas = Canvas::with_size(CARD_COLUMNS + 2 * MARGIN, lines.len() + 2 * MARGIN);
    for (row, line) in lines.iter().enumerate() {
        let mut x = MARGIN;
        for ch in line.text.chars() {
            let at = Pos { x, y: MARGIN + row };
            match line.ink.colour() {
                Some(colour) => canvas.put_glyph_colored(at, ch, colour),
                None => canvas.put_glyph(at, ch),
            };
            x += Canvas::display_width(ch);
        }
    }
    canvas
}

fn width(text: &str) -> usize {
    text.chars().map(Canvas::display_width).sum()
}

/// The longest head of `text` no wider than `columns`, and the rest.
fn split_at_width(text: &str, columns: usize) -> (String, String) {
    let mut used = 0;
    for (index, ch) in text.char_indices() {
        used += Canvas::display_width(ch);
        if used > columns {
            return (text[..index].to_string(), text[index..].to_string());
        }
    }
    (text.to_string(), String::new())
}
