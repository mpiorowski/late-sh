use crate::app::common::primitives::Screen;
use crate::app::help_modal::data::HelpTopic;
use ratatui::layout::Rect;

use crate::app::state::{
    App, DashboardGameToggleTarget, GAME_SELECTION_2048, GAME_SELECTION_LE_WORD,
    GAME_SELECTION_MINESWEEPER, GAME_SELECTION_NES_2048, GAME_SELECTION_NES_BRICK_BREAKER,
    GAME_SELECTION_NES_CONCENTRATION_ROOM, GAME_SELECTION_NES_DABG,
    GAME_SELECTION_NES_ESCAPE_FROM_PONG, GAME_SELECTION_NES_FALLING, GAME_SELECTION_NES_RHDE,
    GAME_SELECTION_NES_SQUIRREL_DOMINO, GAME_SELECTION_NES_THWAITE, GAME_SELECTION_NES_ZAP_RUDER,
    GAME_SELECTION_NONOGRAMS, GAME_SELECTION_RUBIKS_CUBE, GAME_SELECTION_SNAKE,
    GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU, GAME_SELECTION_TETRIS,
};

const LOBBY_GAME_ORDER: [usize; 19] = [
    GAME_SELECTION_2048,
    GAME_SELECTION_TETRIS,
    GAME_SELECTION_SNAKE,
    GAME_SELECTION_LE_WORD,
    GAME_SELECTION_RUBIKS_CUBE,
    GAME_SELECTION_SUDOKU,
    GAME_SELECTION_NONOGRAMS,
    GAME_SELECTION_MINESWEEPER,
    GAME_SELECTION_SOLITAIRE,
    GAME_SELECTION_NES_SQUIRREL_DOMINO,
    GAME_SELECTION_NES_THWAITE,
    GAME_SELECTION_NES_DABG,
    GAME_SELECTION_NES_FALLING,
    GAME_SELECTION_NES_BRICK_BREAKER,
    GAME_SELECTION_NES_ESCAPE_FROM_PONG,
    GAME_SELECTION_NES_RHDE,
    GAME_SELECTION_NES_CONCENTRATION_ROOM,
    GAME_SELECTION_NES_ZAP_RUDER,
    GAME_SELECTION_NES_2048,
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

pub(crate) fn nes_rom_for_selection(selection: usize) -> Option<usize> {
    match selection {
        GAME_SELECTION_NES_SQUIRREL_DOMINO => Some(super::nes_cabinet::state::ROM_SQUIRREL_DOMINO),
        GAME_SELECTION_NES_THWAITE => Some(super::nes_cabinet::state::ROM_THWAITE),
        GAME_SELECTION_NES_DABG => Some(super::nes_cabinet::state::ROM_DABG),
        GAME_SELECTION_NES_FALLING => Some(super::nes_cabinet::state::ROM_FALLING),
        GAME_SELECTION_NES_BRICK_BREAKER => Some(super::nes_cabinet::state::ROM_BRICK_BREAKER),
        GAME_SELECTION_NES_ESCAPE_FROM_PONG => {
            Some(super::nes_cabinet::state::ROM_ESCAPE_FROM_PONG)
        }
        GAME_SELECTION_NES_RHDE => Some(super::nes_cabinet::state::ROM_RHDE),
        GAME_SELECTION_NES_CONCENTRATION_ROOM => {
            Some(super::nes_cabinet::state::ROM_CONCENTRATION_ROOM)
        }
        GAME_SELECTION_NES_ZAP_RUDER => Some(super::nes_cabinet::state::ROM_ZAP_RUDER),
        GAME_SELECTION_NES_2048 => Some(super::nes_cabinet::state::ROM_2048),
        _ => None,
    }
}

pub(crate) fn is_nes_selection(selection: usize) -> bool {
    nes_rom_for_selection(selection).is_some()
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
        } else if app.game_selection == GAME_SELECTION_RUBIKS_CUBE {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.is_playing_game = false;
                return true;
            }
            app.rubiks_cube_state.ensure_current_daily();
            return super::rubiks_cube::input::handle_key(&mut app.rubiks_cube_state, byte);
        } else if app.game_selection == GAME_SELECTION_LE_WORD {
            if byte == b'?' {
                app.le_word_state.close_rules();
                open_global_help(app);
                return true;
            }
            if (byte == 0x1B || byte == b'q' || byte == b'Q') && !app.le_word_state.show_rules {
                app.is_playing_game = false;
                return true;
            }
            return super::le_word::input::handle_key(&mut app.le_word_state, byte);
        } else if is_nes_selection(app.game_selection) {
            if byte == 0x1B || byte == b'q' || byte == b'Q' {
                app.nes_cabinet_state.deactivate();
                app.is_playing_game = false;
                return true;
            }
            return super::nes_cabinet::input::handle_key(&mut app.nes_cabinet_state, byte);
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
                || app.game_selection == GAME_SELECTION_RUBIKS_CUBE
                || app.game_selection == GAME_SELECTION_LE_WORD
                || is_nes_selection(app.game_selection)
                || app.game_selection == GAME_SELECTION_SUDOKU
                || (app.game_selection == GAME_SELECTION_NONOGRAMS
                    && app.nonogram_state.has_puzzles())
                || app.game_selection == GAME_SELECTION_MINESWEEPER
                || app.game_selection == GAME_SELECTION_SOLITAIRE
            {
                if let Some(rom) = nes_rom_for_selection(app.game_selection) {
                    app.nes_cabinet_state.select_rom(rom);
                }
                if app.game_selection == GAME_SELECTION_SUDOKU {
                    app.sudoku_state.ensure_loaded();
                }
                app.is_playing_game = true;
                app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
            }
            true
        }
        _ => false,
    }
}

fn open_global_help(app: &mut App) {
    app.help_modal_state
        .set_keep_composer_focused(app.profile_state.profile().keep_composer_focused);
    app.help_modal_state.open(HelpTopic::Pair);
    app.show_help = true;
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
        } else if app.game_selection == GAME_SELECTION_RUBIKS_CUBE {
            app.rubiks_cube_state.ensure_current_daily();
            return super::rubiks_cube::input::handle_arrow(&mut app.rubiks_cube_state, key);
        } else if app.game_selection == GAME_SELECTION_LE_WORD {
            return super::le_word::input::handle_arrow(&mut app.le_word_state, key);
        } else if is_nes_selection(app.game_selection) {
            return super::nes_cabinet::input::handle_arrow(&mut app.nes_cabinet_state, key);
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

pub(crate) fn handle_event(app: &mut App, event: &crate::app::input::ParsedInput) -> bool {
    let crate::app::input::ParsedInput::Mouse(mouse) = event else {
        return false;
    };

    let area = arcade_content_area(app);
    if app.game_selection == GAME_SELECTION_LE_WORD {
        return super::le_word::input::handle_mouse(&mut app.le_word_state, area, *mouse);
    }

    if app.game_selection == GAME_SELECTION_SOLITAIRE {
        return super::solitaire::input::handle_mouse(&mut app.solitaire_state, area, *mouse);
    }

    if app.game_selection == GAME_SELECTION_MINESWEEPER {
        return super::minesweeper::input::handle_mouse(&mut app.minesweeper_state, area, *mouse);
    }

    false
}

fn arcade_content_area(app: &App) -> Rect {
    let area = Rect::new(0, 0, app.size.0, app.size.1);
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let app_inner = if app.show_aquarium_tray && app.shop_state.entitlements().has_aquarium() {
        let tray = crate::app::hub::aquarium::ui::bottom_tray_area(inner);
        Rect {
            height: inner.height.saturating_sub(tray.height),
            ..inner
        }
    } else {
        inner
    };

    if right_sidebar_visible(app) {
        Rect {
            width: app_inner.width.saturating_sub(24),
            ..app_inner
        }
    } else {
        app_inner
    }
}

fn right_sidebar_visible(app: &App) -> bool {
    if app.show_settings {
        let draft = app.settings_modal_state.draft();
        return crate::app::render::resolve_right_sidebar_enabled(
            draft.right_sidebar_mode,
            &draft.right_sidebar_screens,
            Screen::Arcade,
        );
    }

    let profile = app.profile_state.profile();
    crate::app::render::resolve_right_sidebar_enabled(
        profile.right_sidebar_mode,
        &profile.right_sidebar_screens,
        Screen::Arcade,
    )
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
            GAME_SELECTION_LE_WORD
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_LE_WORD),
            GAME_SELECTION_RUBIKS_CUBE
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_RUBIKS_CUBE),
            GAME_SELECTION_SUDOKU
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_SOLITAIRE),
            GAME_SELECTION_NES_SQUIRREL_DOMINO
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_NES_THWAITE),
            GAME_SELECTION_NES_DABG
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_NES_DABG),
            GAME_SELECTION_NES_FALLING
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_NES_FALLING),
            GAME_SELECTION_NES_BRICK_BREAKER
        );
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_NES_BRICK_BREAKER),
            GAME_SELECTION_NES_ESCAPE_FROM_PONG
        );
        assert_eq!(
            prev_lobby_selection(GAME_SELECTION_SUDOKU),
            GAME_SELECTION_RUBIKS_CUBE
        );
    }

    #[test]
    fn lobby_navigation_wraps_in_rendered_order() {
        assert_eq!(
            next_lobby_selection(GAME_SELECTION_NES_2048),
            GAME_SELECTION_2048
        );
        assert_eq!(
            prev_lobby_selection(GAME_SELECTION_2048),
            GAME_SELECTION_NES_2048
        );
    }
}
