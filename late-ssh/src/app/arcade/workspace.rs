//! Arcade stops on the backtick workspace cycle: daily puzzles with at least
//! one player move that are not solved yet. Daily boards only — they expire
//! at UTC midnight, so abandoned puzzles fall out of the cycle on their own.
//! Real-time score games (Lateris, Snake, Traffic, NES) never join.

use chrono::{NaiveDate, Utc};
use late_core::models::leaderboard::{DailyCompletionStatus, DailyPuzzle};

use crate::app::common::primitives::Screen;
use crate::app::state::{
    App, GAME_SELECTION_LE_WORD, GAME_SELECTION_MINESWEEPER, GAME_SELECTION_NONOGRAMS,
    GAME_SELECTION_RUBIKS_CUBE, GAME_SELECTION_SLIDING_PUZZLE, GAME_SELECTION_SOLITAIRE,
    GAME_SELECTION_SUDOKU,
};

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

/// One cycle-eligible Arcade daily puzzle. Roster order mirrors the Arcade
/// lobby order (`LOBBY_GAME_ORDER` in `arcade/input.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArcadeStop {
    LeWord,
    RubiksCube,
    SlidingPuzzle,
    Sudoku,
    Nonogram,
    Minesweeper,
    Solitaire,
}

impl ArcadeStop {
    pub(crate) const ALL: [ArcadeStop; 7] = [
        ArcadeStop::LeWord,
        ArcadeStop::RubiksCube,
        ArcadeStop::SlidingPuzzle,
        ArcadeStop::Sudoku,
        ArcadeStop::Nonogram,
        ArcadeStop::Minesweeper,
        ArcadeStop::Solitaire,
    ];

    pub(crate) fn game_selection(self) -> usize {
        match self {
            ArcadeStop::LeWord => GAME_SELECTION_LE_WORD,
            ArcadeStop::RubiksCube => GAME_SELECTION_RUBIKS_CUBE,
            ArcadeStop::SlidingPuzzle => GAME_SELECTION_SLIDING_PUZZLE,
            ArcadeStop::Sudoku => GAME_SELECTION_SUDOKU,
            ArcadeStop::Nonogram => GAME_SELECTION_NONOGRAMS,
            ArcadeStop::Minesweeper => GAME_SELECTION_MINESWEEPER,
            ArcadeStop::Solitaire => GAME_SELECTION_SOLITAIRE,
        }
    }

    pub(crate) fn for_selection(selection: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|stop| stop.game_selection() == selection)
    }
}

/// The stop for the active Arcade board, but only when that board is a daily
/// in progress. Personal/practice boards return `None` so backtick never
/// treats them as their game's daily stop (personal boards never join the
/// cycle). Le Word and Rubik's Cube are daily-only; Sliding Puzzle exposes a
/// separate personal mode.
pub(crate) fn active_daily_stop(app: &App) -> Option<ArcadeStop> {
    let stop = ArcadeStop::for_selection(app.game_selection)?;
    let is_daily = match stop {
        ArcadeStop::LeWord | ArcadeStop::RubiksCube => true,
        ArcadeStop::SlidingPuzzle => app.sliding_puzzle_state.is_daily_active(),
        ArcadeStop::Sudoku => app.sudoku_state.is_daily_active(),
        ArcadeStop::Nonogram => app.nonogram_state.is_daily_active(),
        ArcadeStop::Minesweeper => app.minesweeper_state.is_daily_active(),
        ArcadeStop::Solitaire => app.solitaire_state.is_daily_active(),
    };
    is_daily.then_some(stop)
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

/// Arcade stops with an unfinished daily board, in lobby order.
pub(crate) fn unfinished_daily_stops(app: &App) -> Vec<ArcadeStop> {
    ArcadeStop::ALL
        .into_iter()
        .filter(|stop| match stop {
            ArcadeStop::LeWord => app.le_word_state.has_unfinished_daily(),
            ArcadeStop::RubiksCube => app.rubiks_cube_state.has_unfinished_daily(),
            ArcadeStop::SlidingPuzzle => app.sliding_puzzle_state.has_unfinished_daily(),
            ArcadeStop::Sudoku => app.sudoku_state.first_unfinished_daily().is_some(),
            ArcadeStop::Nonogram => app.nonogram_state.first_unfinished_daily().is_some(),
            ArcadeStop::Minesweeper => app.minesweeper_state.first_unfinished_daily().is_some(),
            ArcadeStop::Solitaire => app.solitaire_state.first_unfinished_daily().is_some(),
        })
        .collect()
}

/// Open a stop's unfinished daily board as the active Arcade game. The caller
/// switches the screen; this only points the Arcade at the right board.
pub(crate) fn open_stop(app: &mut App, stop: ArcadeStop) {
    match stop {
        ArcadeStop::LeWord => {}
        ArcadeStop::RubiksCube => {
            app.rubiks_cube_state.ensure_current_daily();
        }
        ArcadeStop::SlidingPuzzle => {
            app.sliding_puzzle_state.ensure_current_daily();
            let index = app
                .sliding_puzzle_state
                .first_unfinished_daily()
                .unwrap_or(0);
            app.sliding_puzzle_state.open_daily(index);
        }
        ArcadeStop::Sudoku => {
            let index = app.sudoku_state.first_unfinished_daily().unwrap_or(0);
            app.sudoku_state.open_daily(index);
        }
        ArcadeStop::Nonogram => {
            let index = app.nonogram_state.first_unfinished_daily().unwrap_or(0);
            app.nonogram_state.open_daily(index);
        }
        ArcadeStop::Minesweeper => {
            let index = app.minesweeper_state.first_unfinished_daily().unwrap_or(0);
            app.minesweeper_state.open_daily(index);
        }
        ArcadeStop::Solitaire => {
            let index = app.solitaire_state.first_unfinished_daily().unwrap_or(0);
            app.solitaire_state.open_daily(index);
        }
    }
    app.game_selection = stop.game_selection();
    app.is_playing_game = true;
}
