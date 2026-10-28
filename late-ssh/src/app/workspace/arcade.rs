//! The Arcade leg of the backtick workspace cycle: daily puzzles with at
//! least one player move that are not solved yet. Daily boards only, since
//! they expire at UTC midnight, so abandoned puzzles fall out of the cycle
//! on their own. Real-time score games (Lateris, Snake, Traffic, NES) never
//! join. A stop is a `DailyPuzzle`, in its roster's (the lobby's) order.

use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle};

use crate::app::state::App;

/// The stop for the active Arcade board, but only when that board is a daily
/// in progress. Personal/practice boards return `None` so backtick never
/// treats them as their game's daily stop (personal boards never join the
/// cycle). Le Word, Rubik's Cube, Word Wheel and Quartets are daily-only;
/// Sliding Puzzle exposes a separate personal mode.
pub(crate) fn active_daily_stop(app: &App) -> Option<DailyPuzzle> {
    let ArcadeGame::Puzzle(puzzle) = app.game_selection else {
        return None;
    };
    let is_daily = match puzzle {
        DailyPuzzle::LeWord
        | DailyPuzzle::RubiksCube
        | DailyPuzzle::WordWheel
        | DailyPuzzle::Quartets => true,
        DailyPuzzle::SlidingPuzzle => app.sliding_puzzle_state.is_daily_active(),
        DailyPuzzle::Sudoku => app.sudoku_state.is_daily_active(),
        DailyPuzzle::Nonogram => app.nonogram_state.is_daily_active(),
        DailyPuzzle::Minesweeper => app.minesweeper_state.is_daily_active(),
        DailyPuzzle::Solitaire => app.solitaire_state.is_daily_active(),
        DailyPuzzle::Netwalk => app.netwalk_state.is_daily_active(),
        DailyPuzzle::ChessPuzzle => app.chess_puzzle_state.is_daily_active(),
    };
    is_daily.then_some(puzzle)
}

/// Dailies with an unfinished board, in lobby order.
pub(crate) fn unfinished_daily_stops(app: &App) -> Vec<DailyPuzzle> {
    DailyPuzzle::ALL
        .iter()
        .copied()
        .filter(|puzzle| has_unfinished_daily(app, *puzzle))
        .collect()
}

fn has_unfinished_daily(app: &App, puzzle: DailyPuzzle) -> bool {
    match puzzle {
        DailyPuzzle::LeWord => app.le_word_state.has_unfinished_daily(),
        DailyPuzzle::RubiksCube => app.rubiks_cube_state.has_unfinished_daily(),
        DailyPuzzle::SlidingPuzzle => app.sliding_puzzle_state.has_unfinished_daily(),
        DailyPuzzle::Sudoku => app.sudoku_state.first_unfinished_daily().is_some(),
        DailyPuzzle::Nonogram => app.nonogram_state.first_unfinished_daily().is_some(),
        DailyPuzzle::Minesweeper => app.minesweeper_state.first_unfinished_daily().is_some(),
        DailyPuzzle::Solitaire => app.solitaire_state.first_unfinished_daily().is_some(),
        DailyPuzzle::Netwalk => app.netwalk_state.has_unfinished_daily(),
        DailyPuzzle::ChessPuzzle => app.chess_puzzle_state.has_unfinished_daily(),
        DailyPuzzle::WordWheel => app.word_wheel_state.has_unfinished_daily(),
        DailyPuzzle::Quartets => app.quartets_state.has_unfinished_daily(),
    }
}

/// Open a stop's unfinished daily board as the active Arcade game. The caller
/// switches the screen; this only points the Arcade at the right board.
pub(crate) fn open_stop(app: &mut App, puzzle: DailyPuzzle) {
    match puzzle {
        DailyPuzzle::LeWord => {}
        DailyPuzzle::RubiksCube => {
            app.rubiks_cube_state.ensure_current_daily();
        }
        DailyPuzzle::SlidingPuzzle => {
            app.sliding_puzzle_state.ensure_current_daily();
            let index = app
                .sliding_puzzle_state
                .first_unfinished_daily()
                .unwrap_or(0);
            app.sliding_puzzle_state.open_daily(index);
        }
        DailyPuzzle::Sudoku => {
            let index = app.sudoku_state.first_unfinished_daily().unwrap_or(0);
            app.sudoku_state.open_daily(index);
        }
        DailyPuzzle::Nonogram => {
            let index = app.nonogram_state.first_unfinished_daily().unwrap_or(0);
            app.nonogram_state.open_daily(index);
        }
        DailyPuzzle::Minesweeper => {
            let index = app.minesweeper_state.first_unfinished_daily().unwrap_or(0);
            app.minesweeper_state.open_daily(index);
        }
        DailyPuzzle::Solitaire => {
            let index = app.solitaire_state.first_unfinished_daily().unwrap_or(0);
            app.solitaire_state.open_daily(index);
        }
        DailyPuzzle::Netwalk => {
            app.netwalk_state.ensure_current_daily();
            let index = app.netwalk_state.first_unfinished_daily().unwrap_or(0);
            app.netwalk_state.open_daily(index);
        }
        DailyPuzzle::ChessPuzzle => {
            app.chess_puzzle_state.ensure_current_daily();
            let index = app.chess_puzzle_state.first_unfinished_daily().unwrap_or(0);
            app.chess_puzzle_state.open_daily(index);
        }
        DailyPuzzle::WordWheel | DailyPuzzle::Quartets => {}
    }
    app.game_selection = ArcadeGame::Puzzle(puzzle);
    app.is_playing_game = true;
}
