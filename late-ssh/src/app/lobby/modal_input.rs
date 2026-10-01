use crate::app::common::primitives::Screen;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::lobby::daily::state::BoardEntry;
use crate::app::lobby::state::LobbyEntry;
use crate::app::state::App;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    // A challenge draft owns the keyboard while open.
    if app.daily.challenge_draft.is_some() {
        handle_draft_input(app, event);
        return;
    }

    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app);
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.lobby.move_selection(&app.daily, 1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.lobby.move_selection(&app.daily, -1);
        }
        // The modal owns input while it is open, so the wheel never reaches
        // the global scroll fallback: move the cursor the way the wheel turns.
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.lobby.move_selection(&app.daily, -1),
            MouseEventKind::ScrollDown => app.lobby.move_selection(&app.daily, 1),
            _ => {}
        },
        ParsedInput::Byte(b'\r' | b'\n' | b' ') | ParsedInput::Char(' ') => {
            activate_selection(app);
        }
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => {
            app.lobby.confirm_claim = None;
            app.daily.begin_challenge_draft();
        }
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
            enum Dismiss {
                Cancel(uuid::Uuid),
                AckResult(uuid::Uuid),
            }
            let action = match app.lobby.selected_entry(&app.daily) {
                Some(LobbyEntry::Challenge(challenge))
                    if challenge.challenger_id == app.daily.user_id() =>
                {
                    Some(Dismiss::Cancel(challenge.id))
                }
                // Acknowledge a result without opening the board.
                Some(LobbyEntry::Finished(item)) => Some(Dismiss::AckResult(item.id)),
                _ => None,
            };
            match action {
                Some(Dismiss::Cancel(match_id)) => app.daily.cancel_challenge(match_id),
                Some(Dismiss::AckResult(match_id)) => app.daily.dismiss_finished(match_id),
                None => {}
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    if app.daily.challenge_draft.is_some() {
        app.daily.draft_back();
        return;
    }
    if app.lobby.confirm_claim.take().is_some() {
        return;
    }
    // Everything visible in the modal has been seen; don't glow for it.
    app.lobby.mark_seen(&app.daily);
    app.show_lobby_modal = false;
}

/// Enter on a match opens its board; Enter on someone else's challenge asks
/// for confirmation, then claims.
fn activate_selection(app: &mut App) {
    enum Action {
        OpenBoard(crate::app::lobby::daily::svc::DailyMatchItem),
        OpenFinished(crate::app::lobby::daily::svc::DailyFinishedItem),
        ConfirmClaim(uuid::Uuid),
        Claim(uuid::Uuid),
        OpenHouseTable(crate::app::lobby::house::tables::HouseTable),
    }
    let action = match app.lobby.selected_entry(&app.daily) {
        Some(LobbyEntry::Match(item)) => Some(Action::OpenBoard(item.clone())),
        // Watching someone else's game opens the same board, read-only.
        Some(LobbyEntry::Spectate(item)) => Some(Action::OpenBoard(item.clone())),
        // Reviewing an unseen result: read-only too (the match is over), and
        // leaving the board acknowledges it.
        Some(LobbyEntry::Finished(item)) => Some(Action::OpenFinished(item.clone())),
        Some(LobbyEntry::Challenge(challenge)) => {
            if challenge.challenger_id == app.daily.user_id() {
                None
            } else if app.lobby.confirm_claim == Some(challenge.id) {
                Some(Action::Claim(challenge.id))
            } else {
                Some(Action::ConfirmClaim(challenge.id))
            }
        }
        Some(LobbyEntry::House(table)) => Some(Action::OpenHouseTable(table)),
        None => None,
    };
    // Switching surfaces while a board or table is already open keeps the
    // original return screen, so Esc never lands on a dead board.
    let return_screen = if app.screen == Screen::DailyMatch {
        app.daily
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(Screen::Dashboard)
    } else if app.screen == Screen::HouseTable {
        app.house.return_screen
    } else {
        app.screen
    };
    match action {
        Some(Action::OpenBoard(item)) => {
            app.daily
                .open_board(&item, return_screen, BoardEntry::Lobby);
            app.show_lobby_modal = false;
            app.set_screen(Screen::DailyMatch);
        }
        Some(Action::OpenFinished(item)) => {
            app.daily
                .open_finished_board(&item, return_screen, BoardEntry::Lobby);
            app.show_lobby_modal = false;
            app.set_screen(Screen::DailyMatch);
        }
        Some(Action::ConfirmClaim(match_id)) => {
            app.lobby.confirm_claim = Some(match_id);
        }
        Some(Action::Claim(match_id)) => {
            app.daily.claim_challenge(match_id);
            app.lobby.confirm_claim = None;
        }
        Some(Action::OpenHouseTable(table)) => {
            if !app.house.enter(table, return_screen, app.chip_balance) {
                app.banner = Some(crate::app::common::primitives::Banner::error(
                    "The table failed to open. Try again in a moment.",
                ));
                return;
            }
            app.show_lobby_modal = false;
            app.set_screen(Screen::HouseTable);
        }
        None => {}
    }
}

/// Keys on the challenge picker overlay: `j`/`k` walk the game list, Esc
/// closes it, Enter posts the picked game.
fn handle_draft_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => {
            app.daily.draft_back();
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.daily.draft_advance();
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.daily.draft_move_selection(1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.daily.draft_move_selection(-1);
        }
        _ => {}
    }
}
