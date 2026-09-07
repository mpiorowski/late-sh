//! The Rubik's Cube share card: the solve as a colour ribbon, one square
//! per face turn, wrapping at twelve per row. Nobody can spoil a cube, so
//! the card is pure signature.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Glyph, MAX_ROW_GLYPHS, ShareCard};

use super::state::{Face, State};

/// Turns per ribbon row: the widest a glyph row may be.
pub const RIBBON_WIDTH: usize = MAX_ROW_GLYPHS;

/// Whether today's cube has been turned and is solved.
pub fn is_ready(state: &State) -> bool {
    state.has_started() && state.is_solved()
}

/// The card for a solved daily cube, or `None` while it is still scrambled.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    Some(card(
        state.puzzle_date(),
        state.user_moves(),
        state.move_log(),
    ))
}

/// `moves` is the full count; `faces` is this session's turn log, which is
/// shorter when the cube was restored from a save. The ribbon shows the
/// last `MAX_ROWS * RIBBON_WIDTH` turns.
pub fn card(puzzle_date: NaiveDate, moves: u32, faces: &[Face]) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::RubiksCube), puzzle_date);
    let result = format!("{moves} moves");
    let glyphs: Vec<Glyph> = faces.iter().map(|face| glyph(*face)).collect();
    ShareCard {
        title: share::title("Rubik's Cube", number, &result),
        rows: share::ribbon(&glyphs, RIBBON_WIDTH),
    }
}

/// Western colour scheme: white up, yellow down, green front, blue back,
/// red right, orange left.
fn glyph(face: Face) -> Glyph {
    match face {
        Face::Up => Glyph::White,
        Face::Down => Glyph::Yellow,
        Face::Front => Glyph::Green,
        Face::Back => Glyph::Blue,
        Face::Right => Glyph::Red,
        Face::Left => Glyph::Orange,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
