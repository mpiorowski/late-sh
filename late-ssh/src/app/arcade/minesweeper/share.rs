//! The Minesweeper share card: no board, that would spoil the mines. The
//! stats line and a strip of the last clicks, safe, flag, or boom, which
//! tells the story on its own.

use chrono::NaiveDate;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Click, MAX_LIVES, Mode, State};

/// How many of the latest clicks the strip shows.
pub const STRIP_CLICKS: usize = 10;

/// Whether a daily field is finished, cleared or boomed. Personal boards
/// never get a card.
pub fn is_ready(state: &State) -> bool {
    state.mode == Mode::Daily && state.is_game_over
}

/// The card for a finished daily, or `None` on a personal board or while
/// the field is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    Some(card(
        state.daily_date(),
        state.difficulty_key(),
        state.mine_count(),
        state.lives,
        state.click_log(),
    ))
}

pub fn card(
    puzzle_date: NaiveDate,
    difficulty_key: &str,
    mines: usize,
    lives: u8,
    clicks: &[Click],
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let outcome = if lives == 0 { "boom" } else { "cleared" };
    let result = format!("{difficulty_key} · {outcome} · {lives}/{MAX_LIVES} lives");
    let start = clicks.len().saturating_sub(STRIP_CLICKS);
    let strip: Vec<Glyph> = clicks[start..].iter().map(|click| glyph(*click)).collect();
    let mut rows = vec![Row::Text(format!("💣 {mines} mines"))];
    if !strip.is_empty() {
        rows.push(Row::Glyphs(strip));
    }
    ShareCard {
        title: share::title("Minesweeper", number, Some(&result)),
        rows,
    }
}

fn glyph(click: Click) -> Glyph {
    match click {
        Click::Safe => Glyph::Green,
        Click::Flag => Glyph::Flag,
        Click::Boom => Glyph::Boom,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
