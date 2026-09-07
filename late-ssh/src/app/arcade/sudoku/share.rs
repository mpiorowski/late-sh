//! The Sudoku share card: the grid with no digits. White is a clue you were
//! given, green is a cell you filled. It reads as a sudoku because it is
//! one, and the difficulty shows itself in how few clues there were.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mask, Mode, State};

/// Whether a daily grid is finished. Personal boards never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode == Mode::Daily && state.is_game_over
}

/// The card for a finished daily, or `None` on a personal board or while
/// the grid is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    Some(card(
        state.daily_date(),
        state.difficulty_key(),
        &state.fixed_mask,
    ))
}

/// `given` is true for a clue cell, false for one the player filled.
pub fn card(puzzle_date: NaiveDate, difficulty_key: &str, given: &Mask) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::Sudoku), puzzle_date);
    let rows = given
        .iter()
        .map(|row| {
            Row::Glyphs(
                row.iter()
                    .map(|is_given| {
                        if *is_given {
                            Glyph::White
                        } else {
                            Glyph::Green
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    ShareCard {
        title: share::title("Sudoku", number, Some(difficulty_key)),
        rows,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
