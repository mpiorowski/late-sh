use crate::app::common::primitives::Banner;
use crate::app::state::{App, DOOR_SELECTION_LATEANIA};

const DOOR_GAME_ORDER: [usize; 1] = [DOOR_SELECTION_LATEANIA];

fn lobby_order_position(selection: usize) -> usize {
    DOOR_GAME_ORDER
        .iter()
        .position(|game| *game == selection)
        .unwrap_or(0)
}

fn next_lobby_selection(selection: usize) -> usize {
    let next = (lobby_order_position(selection) + 1) % DOOR_GAME_ORDER.len();
    DOOR_GAME_ORDER[next]
}

fn prev_lobby_selection(selection: usize) -> usize {
    let pos = lobby_order_position(selection);
    let prev = pos.saturating_add(DOOR_GAME_ORDER.len() - 1) % DOOR_GAME_ORDER.len();
    DOOR_GAME_ORDER[prev]
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.door_delete_confirm {
        return handle_delete_confirm_key(app, byte);
    }

    if app.lateania_state.is_some() {
        return handle_active_lateania_key(app, byte);
    }

    match byte {
        b'j' | b'J' => {
            app.door_game_selection = next_lobby_selection(app.door_game_selection);
            true
        }
        b'k' | b'K' => {
            app.door_game_selection = prev_lobby_selection(app.door_game_selection);
            true
        }
        b'\r' | b'\n' => {
            if app.door_game_selection == DOOR_SELECTION_LATEANIA {
                app.door_delete_confirm = false;
                app.enter_lateania();
            }
            true
        }
        b'd' | b'D' => {
            if app.door_game_selection == DOOR_SELECTION_LATEANIA {
                app.door_delete_confirm = true;
                return true;
            }
            false
        }
        _ => false,
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.door_delete_confirm {
        return true;
    }

    if app.lateania_state.is_some() {
        let Some(state) = app.lateania_state.as_mut() else {
            return true;
        };
        let _ = super::lateania::input::handle_arrow(state, key);
        return true;
    }

    match key {
        b'A' => {
            app.door_game_selection = prev_lobby_selection(app.door_game_selection);
            true
        }
        b'B' => {
            app.door_game_selection = next_lobby_selection(app.door_game_selection);
            true
        }
        _ => false,
    }
}

pub fn leave_active_game(app: &mut App) -> bool {
    if app.door_delete_confirm {
        app.door_delete_confirm = false;
        return true;
    }

    if app.lateania_state.is_some() {
        app.leave_lateania();
        true
    } else {
        false
    }
}

fn handle_delete_confirm_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'y' | b'Y' | b'\r' | b'\n' => {
            app.door_delete_confirm = false;
            app.leave_lateania();
            app.lateania_service.delete_character_task(app.user_id);
            app.banner = Some(Banner::success(
                "Lateania character reset. Enter Lateania to start over.",
            ));
            true
        }
        b'n' | b'N' | b'd' | b'D' | b'q' | b'Q' | 0x1B => {
            app.door_delete_confirm = false;
            true
        }
        _ => true,
    }
}

fn handle_active_lateania_key(app: &mut App, byte: u8) -> bool {
    if byte == 0x1B {
        app.leave_lateania();
        return true;
    }

    let Some(state) = app.lateania_state.as_mut() else {
        return true;
    };
    let _ = super::lateania::input::handle_key(state, byte);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_lobby_entry_stays_selected() {
        assert_eq!(
            next_lobby_selection(DOOR_SELECTION_LATEANIA),
            DOOR_SELECTION_LATEANIA
        );
        assert_eq!(
            prev_lobby_selection(DOOR_SELECTION_LATEANIA),
            DOOR_SELECTION_LATEANIA
        );
    }
}
