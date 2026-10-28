//! The Netwalk share card: the day's scramble as it started, one square per
//! tile, green where the source already reached and dark where it did not,
//! then the move count. The solved board is all green and says nothing, so
//! unlike Sliding Puzzle's card there is no "after".

use chrono::NaiveDate;
use late_core::models::chips::Difficulty;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mode, State, board_dimension, generate, lit_tiles};

/// Whether a daily board has been turned and is solved. Personal boards
/// never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode() == Mode::Daily && state.has_started() && state.is_solved()
}

/// The card for a solved daily board, or `None` on a personal board or
/// while the network is still broken.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    let difficulty = state.difficulty();
    Some(card(
        state.puzzle_date(),
        difficulty,
        state.moves(),
        &generate(difficulty, state.scramble_seed()).scrambled,
    ))
}

/// `scrambled` is the board as the day started, row-major masks.
pub fn card(
    puzzle_date: NaiveDate,
    difficulty: Difficulty,
    moves: u32,
    scrambled: &[u8],
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let dimension = board_dimension(difficulty);
    let result = format!("{} {dimension}×{dimension}", difficulty.key());
    let mut rows: Vec<Row> = lit_tiles(scrambled, dimension)
        .chunks(dimension)
        .map(|row| {
            Row::Glyphs(
                row.iter()
                    .map(|lit| if *lit { Glyph::Green } else { Glyph::Dark })
                    .collect(),
            )
        })
        .collect();
    rows.push(share::arrow_row(moves));
    ShareCard {
        title: share::title("Netwalk", number, Some(&result)),
        rows,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
