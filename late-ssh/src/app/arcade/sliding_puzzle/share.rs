//! The Sliding Puzzle share card: the day's scramble, an arrow with the
//! move count, and the solved board. Tiles are coloured by the row they
//! belong in, so the scramble is a jumble and the solve is clean stripes
//! with the gap in the corner. Before and after, no legend needed.

use chrono::NaiveDate;
use late_core::models::chips::Difficulty;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mode, State, board_dimension, generate_scramble, solved_board};

/// Whether a daily board has been moved and is solved. Personal boards
/// never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode == Mode::Daily && state.has_started() && state.is_solved()
}

/// The card for a solved daily board, or `None` on a personal board or
/// while the tiles are still scrambled.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    let difficulty = state.difficulty();
    Some(card(
        state.puzzle_date(),
        difficulty,
        state.moves(),
        &generate_scramble(difficulty, state.scramble_seed()).tiles,
    ))
}

/// `scrambled` is the board as the day started, row-major, 0 for the gap.
pub fn card(
    puzzle_date: NaiveDate,
    difficulty: Difficulty,
    moves: u32,
    scrambled: &[u8],
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let dimension = board_dimension(difficulty);
    let result = format!("{} {dimension}×{dimension}", difficulty.key());
    let mut rows = board_rows(scrambled, difficulty);
    rows.push(share::arrow_row(moves));
    rows.extend(board_rows(&solved_board(difficulty), difficulty));
    ShareCard {
        title: share::title("Sliding Puzzle", number, Some(&result)),
        rows,
    }
}

fn board_rows(tiles: &[u8], difficulty: Difficulty) -> Vec<Row> {
    let dimension = board_dimension(difficulty);
    tiles
        .chunks(dimension)
        .map(|row| Row::Glyphs(row.iter().map(|tile| glyph(*tile, difficulty)).collect()))
        .collect()
}

/// A tile wears the colour of its home row; the gap is dark.
fn glyph(tile: u8, difficulty: Difficulty) -> Glyph {
    if tile == 0 {
        return Glyph::Dark;
    }
    let home_row = (usize::from(tile) - 1) / board_dimension(difficulty);
    row_colours(difficulty)[home_row]
}

fn row_colours(difficulty: Difficulty) -> &'static [Glyph] {
    match difficulty {
        Difficulty::Easy => &[Glyph::Red, Glyph::Yellow, Glyph::Green],
        Difficulty::Medium => &[Glyph::Red, Glyph::Orange, Glyph::Yellow, Glyph::Green],
        Difficulty::Hard => &[
            Glyph::Red,
            Glyph::Orange,
            Glyph::Yellow,
            Glyph::Green,
            Glyph::Blue,
        ],
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
