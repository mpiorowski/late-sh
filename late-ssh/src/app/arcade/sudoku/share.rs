//! The Sudoku share card: no digits, just the nine boxes coloured by which
//! third of the solve finished them. Every solver gets a different
//! fingerprint on the same puzzle.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Mode, State};

/// The card for a finished daily, or `None` on a personal board or while
/// the grid is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if state.mode != Mode::Daily || !state.is_game_over {
        return None;
    }
    Some(card(
        state.daily_date(),
        state.difficulty_key(),
        &state.box_finish_rank,
    ))
}

/// `box_finish_rank` is per box, row-major: 0 for a box this session never
/// saw complete (a board restored from a save), else the finish order
/// 1..=9. Boxes are coloured by thirds of the ranks actually recorded;
/// unrecorded boxes count as finished first.
pub fn card(puzzle_date: NaiveDate, difficulty_key: &str, box_finish_rank: &[u8; 9]) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::Sudoku), puzzle_date);
    let recorded = box_finish_rank.iter().filter(|rank| **rank > 0).count();
    let rows = (0..3)
        .map(|row| {
            Row::Glyphs(
                (0..3)
                    .map(|col| glyph(box_finish_rank[row * 3 + col], recorded))
                    .collect(),
            )
        })
        .collect();
    ShareCard {
        title: share::title("Sudoku", number, difficulty_key),
        rows,
    }
}

fn glyph(rank: u8, recorded: usize) -> Glyph {
    if rank == 0 || recorded == 0 {
        return Glyph::Green;
    }
    // Rank 1..=recorded split into three equal thirds; a remainder pads
    // the later thirds so the first-finished boxes are always green.
    let third = recorded.div_ceil(3);
    match (rank as usize - 1) / third.max(1) {
        0 => Glyph::Green,
        1 => Glyph::Yellow,
        _ => Glyph::Red,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
