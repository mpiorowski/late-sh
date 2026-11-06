//! The Chess Puzzle share card: one square per move the player had to
//! find, green when found first try and yellow when it took a wrong move
//! first, then the puzzle's theme when it has one. Nothing on it names a
//! square or a piece, so it spoils nothing. A lost puzzle has no card.

use chrono::NaiveDate;
use late_core::{chess_puzzle::Theme, models::chips::Difficulty};

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mode, Outcome, State};

/// Whether a daily puzzle is solved. Personal puzzles never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode() == Mode::Daily && state.outcome() == Some(Outcome::Solved)
}

/// The card for a solved daily, or `None` on a personal, lost or unfinished
/// puzzle.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    Some(card(
        state.puzzle_date(),
        state.difficulty(),
        &state.first_tries(),
        state.theme(),
    ))
}

/// `first_tries` holds one entry per move the player found, in order.
pub fn card(
    puzzle_date: NaiveDate,
    difficulty: Difficulty,
    first_tries: &[bool],
    theme: Option<Theme>,
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let mut rows = vec![Row::Glyphs(
        first_tries
            .iter()
            .map(|first_try| {
                if *first_try {
                    Glyph::Green
                } else {
                    Glyph::Yellow
                }
            })
            .collect(),
    )];
    if let Some(theme) = theme {
        rows.push(Row::Text(theme.label().to_string()));
    }
    ShareCard {
        title: share::title("Chess Puzzle", number, Some(difficulty.key())),
        rows,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
