//! Arcade stops on the backtick workspace cycle: daily puzzles with at least
//! one player move that are not solved yet. Daily boards only — they expire
//! at UTC midnight, so abandoned puzzles fall out of the cycle on their own.
//! Real-time score games (Lateris, Snake, Traffic, NES) never join.

use crate::app::state::{
    App, GAME_SELECTION_LE_WORD, GAME_SELECTION_MINESWEEPER, GAME_SELECTION_NONOGRAMS,
    GAME_SELECTION_RUBIKS_CUBE, GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU,
};

/// One cycle-eligible Arcade daily puzzle. Roster order mirrors the Arcade
/// lobby order (`LOBBY_GAME_ORDER` in `arcade/input.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArcadeStop {
    LeWord,
    RubiksCube,
    Sudoku,
    Nonogram,
    Minesweeper,
    Solitaire,
}

impl ArcadeStop {
    pub(crate) const ALL: [ArcadeStop; 6] = [
        ArcadeStop::LeWord,
        ArcadeStop::RubiksCube,
        ArcadeStop::Sudoku,
        ArcadeStop::Nonogram,
        ArcadeStop::Minesweeper,
        ArcadeStop::Solitaire,
    ];

    pub(crate) fn game_selection(self) -> usize {
        match self {
            ArcadeStop::LeWord => GAME_SELECTION_LE_WORD,
            ArcadeStop::RubiksCube => GAME_SELECTION_RUBIKS_CUBE,
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

/// Arcade stops with an unfinished daily board, in lobby order.
pub(crate) fn unfinished_daily_stops(app: &App) -> Vec<ArcadeStop> {
    ArcadeStop::ALL
        .into_iter()
        .filter(|stop| match stop {
            ArcadeStop::LeWord => app.le_word_state.has_unfinished_daily(),
            ArcadeStop::RubiksCube => app.rubiks_cube_state.has_unfinished_daily(),
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
        ArcadeStop::RubiksCube => app.rubiks_cube_state.ensure_current_daily(),
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
