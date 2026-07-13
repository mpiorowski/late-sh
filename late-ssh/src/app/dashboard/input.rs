use uuid::Uuid;

use crate::app::{
    chat,
    common::primitives::{Banner, Screen},
    state::App,
};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    chat::input::handle_arrow(app, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.music_prefix_armed {
        app.music_prefix_armed = false;
        if crate::app::audio::input::handle_music_suffix(app, byte, true) {
            return true;
        }
    }

    if byte == b'`' {
        return cycle_game_workspace(app);
    }

    if matches!(byte, b'v' | b'V') {
        app.music_prefix_armed = true;
        return true;
    }

    chat::input::handle_byte(app, byte)
}

/// One stop on the backtick cycle: Home chat, or a daily board where it's
/// your move. The cycle is lobby games only — rooms and Arcade dropped out
/// when correspondence play became the front door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameWorkspace {
    Dashboard,
    DailyBoard(Uuid),
}

/// Backtick: hop Home chat -> each match waiting on your move (nearest
/// deadline first) -> back to Home chat.
pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    let current = match app.screen {
        Screen::Dashboard => None,
        Screen::DailyMatch => app.daily.board.as_ref().map(|board| board.match_id),
        _ => return false,
    };
    let my_turn_ids: Vec<Uuid> = app
        .daily
        .my_turn_matches()
        .iter()
        .map(|item| item.id)
        .collect();
    match next_workspace(&my_turn_ids, current) {
        GameWorkspace::Dashboard => {
            if app.screen == Screen::Dashboard {
                app.banner = Some(Banner::error("No matches waiting on your move."));
            } else {
                // Wrap back to Home chat, no modal: this is the chat half of
                // the toggle, not a lobby visit.
                crate::app::daily::board_input::leave_board(app, Screen::Dashboard);
            }
            true
        }
        GameWorkspace::DailyBoard(match_id) => {
            // Preserve where the first board in the hop chain was opened
            // from so `q`/`Esc` still returns there after any number of
            // backtick hops.
            let return_screen = app
                .daily
                .board
                .as_ref()
                .map(|board| board.return_screen)
                .unwrap_or(Screen::Dashboard);
            let Some(item) = app
                .daily
                .my_turn_matches()
                .into_iter()
                .find(|item| item.id == match_id)
                .cloned()
            else {
                return true;
            };
            app.daily.open_board(&item, return_screen);
            app.set_screen(Screen::DailyMatch);
            true
        }
    }
}

/// The stop after `current` in `[Home, boards...]`. `None` means Home. A
/// current board missing from the list (the turn just passed to the
/// opponent) restarts from the front so the hop chain keeps draining the
/// queue instead of bailing home early.
fn next_workspace(my_turn_ids: &[Uuid], current: Option<Uuid>) -> GameWorkspace {
    let next = match current {
        None => my_turn_ids.first(),
        Some(current_id) => match my_turn_ids.iter().position(|id| *id == current_id) {
            Some(index) => my_turn_ids.get(index + 1),
            None => my_turn_ids.first(),
        },
    };
    match next {
        Some(id) => GameWorkspace::DailyBoard(*id),
        None => GameWorkspace::Dashboard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn from_home_enters_first_board() {
        assert_eq!(
            next_workspace(&[id(1), id(2)], None),
            GameWorkspace::DailyBoard(id(1))
        );
    }

    #[test]
    fn from_home_with_no_matches_stays_home() {
        assert_eq!(next_workspace(&[], None), GameWorkspace::Dashboard);
    }

    #[test]
    fn advances_through_boards_then_wraps_home() {
        let ids = [id(1), id(2)];
        assert_eq!(
            next_workspace(&ids, Some(id(1))),
            GameWorkspace::DailyBoard(id(2))
        );
        assert_eq!(next_workspace(&ids, Some(id(2))), GameWorkspace::Dashboard);
    }

    #[test]
    fn board_no_longer_my_turn_restarts_from_front() {
        // Just moved on match 1: it left the my-turn list, so the next hop
        // goes to the front of what's still waiting.
        assert_eq!(
            next_workspace(&[id(2), id(3)], Some(id(1))),
            GameWorkspace::DailyBoard(id(2))
        );
    }

    #[test]
    fn last_board_gone_and_queue_empty_lands_home() {
        assert_eq!(next_workspace(&[], Some(id(1))), GameWorkspace::Dashboard);
    }
}
