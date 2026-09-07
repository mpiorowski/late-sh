//! The Nonogram share card: the finished picture in half-blocks. The
//! picture is the solution, and that is accepted on purpose: it is the one
//! card that gives something away, because the picture is the whole brag.
//! The hard 20x20 is what sets the card's ten-row cap.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, ShareCard};

use super::state::{Mode, State};

/// Whether a daily board is finished. Personal boards never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode == Mode::Daily && state.is_game_over()
}

/// The card for a finished daily, or `None` on a personal board or while
/// the puzzle is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    let filled: Vec<Vec<bool>> = state
        .player_grid()
        .iter()
        .map(|row| row.iter().map(|cell| *cell == 1).collect())
        .collect();
    Some(card(state.daily_date(), state.difficulty_key(), &filled))
}

pub fn card(puzzle_date: NaiveDate, difficulty_key: &str, filled: &[Vec<bool>]) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::Nonogram), puzzle_date);
    let height = filled.len();
    let width = filled.first().map_or(0, Vec::len);
    let result = format!("{difficulty_key} {width}×{height}");
    ShareCard {
        title: share::title("Nonograms", number, &result),
        rows: share::half_block_picture(filled),
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
