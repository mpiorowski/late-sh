//! The Nonogram share card: the finished picture in half-blocks. The
//! picture is the solution, and that is accepted on purpose: copying it by
//! hand into clue-checked cells is more work than solving, and the picture
//! is the whole brag.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, ShareCard};

use super::state::{Mode, State};

/// The card for a finished daily, or `None` on a personal board or while
/// the puzzle is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if state.mode != Mode::Daily || !state.is_game_over() {
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
