use crate::app::common::primitives::Screen;
use crate::app::state::{
    App, DashboardGameToggleTarget, GAME_SELECTION_2048, GAME_SELECTION_MINESWEEPER,
    GAME_SELECTION_NONOGRAMS, GAME_SELECTION_SNAKE, GAME_SELECTION_SOLITAIRE,
    GAME_SELECTION_SUDOKU, GAME_SELECTION_TETRIS,
};

const LOBBY_GAME_ORDER: [usize; 7] = [
    GAME_SELECTION_2048,
    GAME_SELECTION_TETRIS,
    GAME_SELECTION_SNAKE,
    GAME_SELECTION_SUDOKU,
    GAME_SELECTION_NONOGRAMS,
    GAME_SELECTION_MINESWEEPER,
    GAME_SELECTION_SOLITAIRE,
];

fn lobby_order_position(selection: usize) -> usize {
    LOBBY_GAME_ORDER
        .iter()
        .position(|game| *game == selection)
        .unwrap_or(0)
}

fn next_lobby_selection(selection: usize) -> usize {
    let next = (lobby_order_position(selection) + 1) % LOBBY_GAME_ORDER.len();
    LOBBY_GAME_ORDER[next]
}

fn prev_lobby_selection(selection: usize) -> usize {
    let pos = lobby_order_position(selection);
    let prev = pos.saturating_add(LOBBY_GAME_ORDER.len() - 1) % LOBBY_GAME_ORDER.len();
    LOBBY_GAME_ORDER[prev]
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.is_playing_game {
        if byte == b'`' {
            app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
            app.set_screen(Screen::Dashboard);
            return true;
        }

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
        } else if app.game_selection == GAME_SELECTION_SNAKE {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.snake_state.persist_progress();
                app.is_playing_game = false;
                return true;
            }
            return super::snake::input::handle_key(&mut app.snake_state, byte);
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
        }
        return false;
    }

    // Lobby mode
    match byte {
        b'j' | b'J' => {
            app.game_selection = next_lobby_selection(app.game_selection);
            true
        }
        b'k' | b'K' => {
            app.game_selection = prev_lobby_selection(app.game_selection);
            true
        }
        b'\r' | b'\n' => {
            if app.game_selection == GAME_SELECTION_2048
                || app.game_selection == GAME_SELECTION_TETRIS
                || app.game_selection == GAME_SELECTION_SNAKE
                || app.game_selection == GAME_SELECTION_SUDOKU
                || (app.game_selection == GAME_SELECTION_NONOGRAMS
                    && app.nonogram_state.has_puzzles())
                || app.game_selection == GAME_SELECTION_MINESWEEPER
                || app.game_selection == GAME_SELECTION_SOLITAIRE
            {
                app.is_playing_game = true;
                app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
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
        } else if app.game_selection == GAME_SELECTION_SNAKE {
            return super::snake::input::handle_arrow(&mut app.snake_state, key);
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
            app.game_selection = prev_lobby_selection(app.game_selection);
            true
        }
        b'B' => {
            // Down
            app.game_selection = next_lobby_selection(app.game_selection);
            true
        }
        _ => false,
    }
}

pub(crate) fn handle_event(_app: &mut App, event: &crate::app::input::ParsedInput) -> bool {
    let _ = event;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lobby_navigation_follows_rendered_order() {
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_2048),
            GAME_SELECTION_TETRIS
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_TETRIS),
            GAME_SELECTION_SNAKE
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_SNAKE),
            GAME_SELECTION_SUDOKU
        );
        assert_eq!(
            prev_lobby_selection(GAME_SELECTION_SUDOKU),
            GAME_SELECTION_SNAKE
        );
    }

    #[test]
    fn lobby_navigation_wraps_in_rendered_order() {
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_SOLITAIRE),
            GAME_SELECTION_2048
        );
        assert_eq!(
            prev_lobby_selection(GAME_SELECTION_2048),
            GAME_SELECTION_SOLITAIRE
        );
    }
}
