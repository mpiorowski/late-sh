//! Arcade daily lifecycle: this session's own win marks for the lobby cards,
//! and the UTC-midnight rollover that swaps every daily board to today's
//! puzzle. The backtick-cycle leg (which dailies count as stops, and how to
//! open one) lives in `workspace/arcade.rs`.

use chrono::{NaiveDate, Utc};
use late_core::models::leaderboard::{DailyCompletionStatus, DailyPuzzle};

use crate::app::common::primitives::Screen;
use crate::app::state::App;

/// The dailies this session banked today, kept beside the leaderboard
/// snapshot so the lobby card turns green the moment a win lands instead of
/// on the next five-minute refresh (`LeaderboardService::REFRESH_INTERVAL`).
/// It is fed from the session's own `GameWon` Activity events, which every
/// daily service publishes only after the win row commits, so it never shows
/// a win the database does not have. Wins stamped on any day but today are
/// ignored: a board finished across UTC midnight belongs to yesterday's card.
pub(crate) struct SessionDailyWins {
    date: NaiveDate,
    status: DailyCompletionStatus,
}

impl SessionDailyWins {
    pub(crate) fn new() -> Self {
        Self {
            date: Utc::now().date_naive(),
            status: DailyCompletionStatus::default(),
        }
    }

    /// Record a win of `game` at `difficulty_key` stamped `won_on`. Returns
    /// true when today's marks changed.
    pub(crate) fn note_win(
        &mut self,
        won_on: NaiveDate,
        game: DailyPuzzle,
        difficulty_key: String,
    ) -> bool {
        let today = Utc::now().date_naive();
        if won_on != today {
            return false;
        }
        if self.date != today {
            self.date = today;
            self.status = DailyCompletionStatus::default();
        }
        if self.status.completed_difficulty(game, &difficulty_key) {
            return false;
        }
        self.status.mark_completed(game, difficulty_key);
        true
    }

    /// Today's session-banked wins, or `None` once the UTC date has moved on.
    pub(crate) fn today(&self) -> Option<&DailyCompletionStatus> {
        (self.date == Utc::now().date_naive()).then_some(&self.status)
    }
}

/// Roll every Arcade daily over to today's puzzle. Reconnecting rebuilds them
/// all, so this is what a session that stays up across UTC midnight needs: it
/// used to keep serving yesterday's boards (and quietly save progress on them
/// under today's date) until the client quit and rejoined, while the quest
/// strip beside them had already rolled.
///
/// Skipped only while the player is looking at a board, so a puzzle is never
/// swapped out mid-move; it rolls on the next tick once they look away.
/// `is_playing_game` alone would not do: it stays set when a board is left
/// open behind a page switch, which would park that session on yesterday's
/// puzzles for good. Returns true when anything moved, so the caller can
/// force a frame.
pub(crate) fn refresh_daily_games(app: &mut App) -> bool {
    if app.screen == Screen::Arcade && app.is_playing_game {
        return false;
    }
    let mut changed = app.le_word_state.ensure_current_daily();
    changed |= app.rubiks_cube_state.ensure_current_daily();
    changed |= app.sliding_puzzle_state.ensure_current_daily();
    changed |= app.sudoku_state.ensure_current_daily();
    changed |= app.nonogram_state.ensure_current_daily();
    changed |= app.minesweeper_state.ensure_current_daily();
    changed |= app.solitaire_state.ensure_current_daily();
    changed
}
