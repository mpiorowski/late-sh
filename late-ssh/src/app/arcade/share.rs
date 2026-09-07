//! Share cards: the spoiler-free result a player pastes wherever they
//! already talk. Every card has the same shape, a `late.sh <Game> #<n>`
//! header, at most eight body rows, and the footer `ssh late.sh`, never a
//! URL, because the command is the brand and the filter at once.
//!
//! This module owns the grammar (the card struct, the closed glyph set, the
//! renderer, the puzzle numbering) and the day card. Each daily builds its
//! own card in a pure `share.rs` beside its `state.rs`; copying, posting,
//! banners, and telemetry stay in `arcade/input.rs`.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

/// The footer of every card. Never a URL.
pub const FOOTER: &str = "ssh late.sh";

/// The widest a glyph row may be, so a card fits a phone screenshot.
pub const MAX_ROW_GLYPHS: usize = 12;

/// The most body rows a card may carry.
pub const MAX_ROWS: usize = 8;

/// How a card is written out. Emoji renders on every social network; ASCII
/// is for people who post in monospace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShareFormat {
    Emoji,
    Ascii,
}

/// The closed set of cells a card body may use. Each game maps its own
/// meaning onto these; the renderer only knows how each one prints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Glyph {
    Green,
    Yellow,
    Orange,
    Red,
    Blue,
    White,
    Dark,
    Flag,
    Boom,
}

impl Glyph {
    pub const fn emoji(self) -> &'static str {
        match self {
            Self::Green => "🟩",
            Self::Yellow => "🟨",
            Self::Orange => "🟧",
            Self::Red => "🟥",
            Self::Blue => "🟦",
            Self::White => "⬜",
            Self::Dark => "⬛",
            Self::Flag => "🚩",
            Self::Boom => "💥",
        }
    }

    pub const fn ascii(self) -> char {
        match self {
            Self::Green => '#',
            Self::Yellow => '+',
            Self::Orange => 'o',
            Self::Red => 'X',
            Self::Blue => '@',
            Self::White => '-',
            Self::Dark => '.',
            Self::Flag => 'F',
            Self::Boom => '*',
        }
    }
}

/// One body row: a run of glyphs, or plain text that prints the same in
/// both formats (a stat line, a half-block picture, a card-suit bar).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Row {
    Glyphs(Vec<Glyph>),
    Text(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareCard {
    /// The header, e.g. `late.sh Le Word #214 · 4/6`.
    pub title: String,
    pub rows: Vec<Row>,
}

/// Which card was shared. Closed so the metric label set is finite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShareCardKind {
    LeWord,
    Nonogram,
    Sudoku,
    Minesweeper,
    Solitaire,
    RubiksCube,
    SlidingPuzzle,
    Day,
}

/// Where a card went.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShareSurface {
    Clipboard,
    Room,
}

/// Render a card as the text the player pastes.
pub fn render(card: &ShareCard, format: ShareFormat) -> String {
    let mut out = String::new();
    out.push_str(&card.title);
    out.push('\n');
    for row in &card.rows {
        match row {
            Row::Glyphs(glyphs) => match format {
                ShareFormat::Emoji => {
                    for glyph in glyphs {
                        out.push_str(glyph.emoji());
                    }
                }
                ShareFormat::Ascii => {
                    for glyph in glyphs {
                        out.push(glyph.ascii());
                    }
                }
            },
            Row::Text(text) => out.push_str(text),
        }
        out.push('\n');
    }
    out.push_str(FOOTER);
    out
}

/// The header line: `late.sh <Game> #<n> · <result>`.
pub fn title(game: &str, number: i64, result: &str) -> String {
    format!("late.sh {game} #{number} · {result}")
}

/// The first day each daily ran. Puzzle numbers count from here so two
/// people's cards from the same day match. Never move one: every card
/// already pasted would renumber.
pub const fn epoch(puzzle: DailyPuzzle) -> NaiveDate {
    match puzzle {
        DailyPuzzle::Sudoku
        | DailyPuzzle::Nonogram
        | DailyPuzzle::Minesweeper
        | DailyPuzzle::Solitaire => DAY_EPOCH,
        DailyPuzzle::LeWord | DailyPuzzle::RubiksCube => date(2026, 6, 18),
        DailyPuzzle::SlidingPuzzle => date(2026, 8, 30),
    }
}

/// The day card counts from the first day any daily ran.
pub const DAY_EPOCH: NaiveDate = date(2026, 4, 11);

/// Days since `epoch`, one-based: the epoch day is puzzle #1.
pub fn puzzle_number(epoch: NaiveDate, day: NaiveDate) -> i64 {
    (day - epoch).num_days() + 1
}

/// The lobby order of the dailies, which is also the order of the day
/// card's row. Keep in sync with `LOBBY_GAME_ORDER` in `arcade/input.rs`.
pub const DAY_CARD_ORDER: [DailyPuzzle; 7] = [
    DailyPuzzle::LeWord,
    DailyPuzzle::RubiksCube,
    DailyPuzzle::SlidingPuzzle,
    DailyPuzzle::Sudoku,
    DailyPuzzle::Nonogram,
    DailyPuzzle::Minesweeper,
    DailyPuzzle::Solitaire,
];

/// The day card: one glyph per daily, filled for each won today, plus the
/// daily-quest streak. `won` answers for each puzzle in `DAY_CARD_ORDER`.
pub fn day_card(day: NaiveDate, won: impl Fn(DailyPuzzle) -> bool, streak_days: i32) -> ShareCard {
    let marks: Vec<Glyph> = DAY_CARD_ORDER
        .iter()
        .map(|puzzle| if won(*puzzle) { Glyph::Green } else { Glyph::Dark })
        .collect();
    let won_count = marks.iter().filter(|g| **g == Glyph::Green).count();
    let number = puzzle_number(DAY_EPOCH, day);
    let result = if streak_days > 0 {
        format!("{won_count}/{} · 🔥 {streak_days}", DAY_CARD_ORDER.len())
    } else {
        format!("{won_count}/{}", DAY_CARD_ORDER.len())
    };
    ShareCard {
        title: title("Daily", number, &result),
        rows: vec![Row::Glyphs(marks)],
    }
}

/// Wrap a run of glyphs into rows of `per_row`, keeping only the last
/// `MAX_ROWS * per_row` so a long history still fits the card.
pub fn ribbon(glyphs: &[Glyph], per_row: usize) -> Vec<Row> {
    let keep = MAX_ROWS * per_row;
    let start = glyphs.len().saturating_sub(keep);
    glyphs[start..]
        .chunks(per_row)
        .map(|chunk| Row::Glyphs(chunk.to_vec()))
        .collect()
}

/// A 0/1 picture as half-block text, two picture rows per text row, so a
/// 10x10 fits in five rows. `filled` is true for a dark cell.
pub fn half_block_picture(filled: &[Vec<bool>]) -> Vec<Row> {
    filled
        .chunks(2)
        .map(|pair| {
            let top = &pair[0];
            let bottom = pair.get(1);
            let text: String = top
                .iter()
                .enumerate()
                .map(|(col, &t)| {
                    let b = bottom.is_some_and(|row| row.get(col).copied().unwrap_or(false));
                    match (t, b) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }
                })
                .collect();
            Row::Text(text.trim_end().to_string())
        })
        .collect()
}

const fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    match NaiveDate::from_ymd_opt(year, month, day) {
        Some(d) => d,
        None => panic!("invalid epoch date"),
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
