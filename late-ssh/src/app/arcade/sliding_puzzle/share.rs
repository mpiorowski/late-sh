//! The Sliding Puzzle share card: a heatmap of where the blank tile spent
//! its time, plus the move count. The board itself is the same for
//! everyone, so the path is the only thing that is yours.

use chrono::NaiveDate;
use late_core::models::chips::Difficulty;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mode, State, board_dimension};

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
    Some(card(
        state.puzzle_date(),
        state.difficulty(),
        state.moves(),
        state.blank_visits(),
    ))
}

/// `visits` is per cell, row-major, `dimension * dimension` long. Heat is
/// relative to the most-visited cell: never visited white, then yellow,
/// orange, red by thirds of that maximum.
pub fn card(
    puzzle_date: NaiveDate,
    difficulty: Difficulty,
    moves: u32,
    visits: &[u32],
) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::SlidingPuzzle), puzzle_date);
    let dimension = board_dimension(difficulty);
    let result = format!(
        "{} {dimension}×{dimension} · {moves} moves",
        difficulty.key()
    );
    let max = visits.iter().copied().max().unwrap_or(0);
    let rows = visits
        .chunks(dimension)
        .map(|row| Row::Glyphs(row.iter().map(|count| glyph(*count, max)).collect()))
        .collect();
    ShareCard {
        title: share::title("Sliding Puzzle", number, &result),
        rows,
    }
}

fn glyph(count: u32, max: u32) -> Glyph {
    if count == 0 || max == 0 {
        return Glyph::White;
    }
    // Thirds of the maximum, rounded up so the top cell is always red.
    let third = max.div_ceil(3).max(1);
    match (count - 1) / third {
        0 => Glyph::Yellow,
        1 => Glyph::Orange,
        _ => Glyph::Red,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
