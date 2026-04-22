use crate::app::state::{
    App, GAME_SELECTION_2048, GAME_SELECTION_BLACKJACK, GAME_SELECTION_MINESWEEPER,
    GAME_SELECTION_NONOGRAMS, GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU,
    GAME_SELECTION_TETRIS,
};

const LOBBY_GAME_COUNT: usize = 7;

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.is_playing_game {
        if app.game_selection == GAME_SELECTION_2048 {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                // Exit game mode back to lobby
                app.is_playing_game = false;
                return true;
            }
            return super::twenty_forty_eight::input::handle_key(
                &mut app.twenty_forty_eight_state,
                byte,
            );
        } else if app.game_selection == GAME_SELECTION_TETRIS {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            return super::tetris::input::handle_key(&mut app.tetris_state, byte);
        } else if app.game_selection == GAME_SELECTION_SUDOKU {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            return super::sudoku::input::handle_key(&mut app.sudoku_state, byte);
        } else if app.game_selection == GAME_SELECTION_NONOGRAMS {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            return super::nonogram::input::handle_key(&mut app.nonogram_state, byte);
        } else if app.game_selection == GAME_SELECTION_MINESWEEPER {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            return super::minesweeper::input::handle_key(&mut app.minesweeper_state, byte);
        } else if app.game_selection == GAME_SELECTION_SOLITAIRE {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            return super::solitaire::input::handle_key(&mut app.solitaire_state, byte);
        } else if app.game_selection == GAME_SELECTION_BLACKJACK {
            let action = if byte == b'q' || byte == b'Q' {
                super::blackjack::input::handle_key(&mut app.blackjack_state, 0x1B)
            } else {
                super::blackjack::input::handle_key(&mut app.blackjack_state, byte)
            };
            match action {
                super::blackjack::input::InputAction::Ignored => return false,
                super::blackjack::input::InputAction::Handled => return true,
                super::blackjack::input::InputAction::Leave => {
                    app.is_playing_game = false;
                    return true;
                }
            }
        }
        return false;
    }

    // Lobby mode
    match byte {
        b'j' | b'J' => {
            app.game_selection = (app.game_selection + 1) % LOBBY_GAME_COUNT;
            true
        }
        b'k' | b'K' => {
            app.game_selection =
                app.game_selection.saturating_add(LOBBY_GAME_COUNT - 1) % LOBBY_GAME_COUNT;
            true
        }
        b'\r' | b'\n' => {
            if app.game_selection == GAME_SELECTION_2048
                || app.game_selection == GAME_SELECTION_TETRIS
                || app.game_selection == GAME_SELECTION_SUDOKU
                || (app.game_selection == GAME_SELECTION_NONOGRAMS
                    && app.nonogram_state.has_puzzles())
                || app.game_selection == GAME_SELECTION_MINESWEEPER
                || app.game_selection == GAME_SELECTION_SOLITAIRE
                || (app.game_selection == GAME_SELECTION_BLACKJACK && app.is_admin)
            {
                app.is_playing_game = true;
            }
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.is_playing_game {
        if app.game_selection == GAME_SELECTION_2048 {
            return super::twenty_forty_eight::input::handle_arrow(
                &mut app.twenty_forty_eight_state,
                key,
            );
        } else if app.game_selection == GAME_SELECTION_TETRIS {
            return super::tetris::input::handle_arrow(&mut app.tetris_state, key);
        } else if app.game_selection == GAME_SELECTION_SUDOKU {
            return super::sudoku::input::handle_arrow(&mut app.sudoku_state, key);
        } else if app.game_selection == GAME_SELECTION_NONOGRAMS {
            return super::nonogram::input::handle_arrow(&mut app.nonogram_state, key);
        } else if app.game_selection == GAME_SELECTION_MINESWEEPER {
            return super::minesweeper::input::handle_arrow(&mut app.minesweeper_state, key);
        } else if app.game_selection == GAME_SELECTION_SOLITAIRE {
            return super::solitaire::input::handle_arrow(&mut app.solitaire_state, key);
        }
        return false;
    }

    // Lobby mode
    match key {
        b'A' => {
            // Up
            app.game_selection =
                app.game_selection.saturating_add(LOBBY_GAME_COUNT - 1) % LOBBY_GAME_COUNT;
            true
        }
        b'B' => {
            // Down
            app.game_selection = (app.game_selection + 1) % LOBBY_GAME_COUNT;
            true
        }
        _ => false,
    }
}

pub(crate) fn handle_event(_app: &mut App, event: &crate::app::input::ParsedInput) -> bool {
    let _ = event;
    false
}
