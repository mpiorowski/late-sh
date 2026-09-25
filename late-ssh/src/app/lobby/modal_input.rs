use crate::app::common::primitives::Screen;
use crate::app::input::{MouseEventKind, ParsedInput};
use crate::app::lobby::state::LobbyEntry;
use crate::app::state::App;

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    // A challenge draft owns the keyboard while open.
    if app.daily.challenge_draft.is_some() {
        handle_draft_input(app, event);
        return;
    }
    // Same for the realm create-game overlay.
    if app.realm.create_draft.is_some() {
        handle_realm_draft_input(app, event);
        return;
    }
    // And for the colour picker that stands between "join" and joining.
    if app.realm.join_draft.is_some() {
        handle_realm_join_input(app, event);
        return;
    }

    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            handle_escape(app);
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.lobby.move_selection(&app.daily, &app.realm, 1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.lobby.move_selection(&app.daily, &app.realm, -1);
        }
        // The modal owns input while it is open, so the wheel never reaches
        // the global scroll fallback: move the cursor the way the wheel turns.
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.lobby.move_selection(&app.daily, &app.realm, -1),
            MouseEventKind::ScrollDown => app.lobby.move_selection(&app.daily, &app.realm, 1),
            _ => {}
        },
        ParsedInput::Byte(b'\r' | b'\n' | b' ') | ParsedInput::Char(' ') => {
            activate_selection(app);
        }
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => {
            app.lobby.confirm_claim = None;
            app.daily.begin_challenge_draft(false);
        }
        ParsedInput::Byte(b'C') | ParsedInput::Char('C') => {
            app.lobby.confirm_claim = None;
            app.daily.begin_challenge_draft(true);
        }
        // Watch without committing: a realm you could join is exactly the
        // one you most want to look at first.
        ParsedInput::Byte(b'w' | b'W') | ParsedInput::Char('w' | 'W') => {
            let watching = match app.lobby.selected_entry(&app.daily, &app.realm) {
                Some(LobbyEntry::Realm(game)) => Some(game.id),
                _ => None,
            };
            if let Some(game_id) = watching {
                app.lobby.confirm_realm = None;
                open_realm_board(app, game_id);
            }
        }
        ParsedInput::Byte(b'n' | b'N') | ParsedInput::Char('n' | 'N') => {
            app.lobby.confirm_claim = None;
            app.lobby.confirm_realm = None;
            app.realm.begin_create_draft();
        }
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
            enum Dismiss {
                Cancel(uuid::Uuid),
                AckResult(uuid::Uuid),
                LeaveRealm(uuid::Uuid),
            }
            let action = match app.lobby.selected_entry(&app.daily, &app.realm) {
                Some(LobbyEntry::Challenge(challenge))
                    if challenge.challenger_id == app.daily.user_id() =>
                {
                    Some(Dismiss::Cancel(challenge.id))
                }
                // Acknowledge a result without opening the board.
                Some(LobbyEntry::Finished(item)) => Some(Dismiss::AckResult(item.id)),
                // Withdrawing is for your first day only (or for the last
                // player packing the whole realm up); the service is the
                // authority, this just offers it when it can.
                Some(LobbyEntry::Realm(game))
                    if game.is_member(app.realm.user_id)
                        && game.winner_user_id.is_none()
                        && game.can_withdraw =>
                {
                    Some(Dismiss::LeaveRealm(game.id))
                }
                _ => None,
            };
            match action {
                Some(Dismiss::Cancel(match_id)) => app.daily.cancel_challenge(match_id),
                Some(Dismiss::AckResult(match_id)) => app.daily.dismiss_finished(match_id),
                Some(Dismiss::LeaveRealm(game_id)) => {
                    let user_id = app.realm.user_id;
                    app.realm.service().leave_game_task(user_id, game_id);
                }
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
    if app.realm.join_cancel() {
        return;
    }
    // The realm overlay peels its own steps before it closes.
    if app.realm.create_draft.is_some() {
        app.realm.draft_back();
        return;
    }
    if app.lobby.confirm_claim.take().is_some() {
        return;
    }
    if app.lobby.confirm_realm.take().is_some() {
        return;
    }
    // Everything visible in the modal has been seen; don't glow for it.
    app.lobby.mark_seen(&app.daily, &app.realm);
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
        OpenRealm(uuid::Uuid),
        ConfirmRealm(uuid::Uuid),
        JoinRealm(crate::app::lobby::realm::svc::RealmGameItem),
    }
    let action = match app.lobby.selected_entry(&app.daily, &app.realm) {
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
        Some(LobbyEntry::Realm(game)) => {
            // A realm is always live, so there is nothing to start: you are
            // either in it (play), able to arrive (join, behind a confirm),
            // or watching.
            // Members play, everyone else watches — unless there is still
            // room to arrive, which is a join behind a confirm.
            if game.is_member(app.realm.user_id) || !game.joinable {
                Some(Action::OpenRealm(game.id))
            } else if app.lobby.confirm_realm == Some(game.id) {
                Some(Action::JoinRealm(game.clone()))
            } else {
                Some(Action::ConfirmRealm(game.id))
            }
        }
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
    } else if app.screen == Screen::Realm {
        app.realm
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(Screen::Dashboard)
    } else {
        app.screen
    };
    match action {
        Some(Action::OpenBoard(item)) => {
            app.daily.open_board(&item, return_screen);
            app.show_lobby_modal = false;
            app.set_screen(Screen::DailyMatch);
        }
        Some(Action::OpenFinished(item)) => {
            app.daily.open_finished_board(&item, return_screen);
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
        Some(Action::OpenRealm(game_id)) => {
            app.realm.open_board(game_id, return_screen);
            app.show_lobby_modal = false;
            app.set_screen(Screen::Realm);
        }

        Some(Action::ConfirmRealm(game_id)) => {
            app.lobby.confirm_realm = Some(game_id);
        }
        // Arriving is a choice of colour, and the colour is how everyone
        // will know you on the map — so the second Enter opens the picker
        // and the join itself goes from there.
        Some(Action::JoinRealm(game)) => {
            app.realm.begin_join_draft(&game);
            app.lobby.confirm_realm = None;
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

/// Keys on the challenge picker overlay. The picker step navigates the game
/// list; the directed username step owns printable input (so `j`/`k` type,
/// they don't scroll). Esc steps back, Enter advances/posts.
fn handle_draft_input(app: &mut App, event: ParsedInput) {
    let username_stage = app
        .daily
        .challenge_draft
        .as_ref()
        .is_some_and(|draft| draft.username.is_some());
    match event {
        ParsedInput::Byte(0x1B) => {
            app.daily.draft_back();
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.daily.draft_advance();
        }
        _ if username_stage => match event {
            ParsedInput::Byte(0x7F | 0x08) => {
                if let Some(buffer) = draft_username_buffer(app) {
                    buffer.pop();
                }
            }
            ParsedInput::Byte(byte) if byte.is_ascii_graphic() => {
                push_prompt_char(app, byte as char);
            }
            ParsedInput::Char(ch) if !ch.is_control() => {
                push_prompt_char(app, ch);
            }
            _ => {}
        },
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

fn draft_username_buffer(app: &mut App) -> Option<&mut String> {
    app.daily
        .challenge_draft
        .as_mut()
        .and_then(|draft| draft.username.as_mut())
}

fn push_prompt_char(app: &mut App, ch: char) {
    const MAX_USERNAME_PROMPT: usize = 32;
    if let Some(buffer) = draft_username_buffer(app)
        && buffer.chars().count() < MAX_USERNAME_PROMPT
    {
        buffer.push(ch);
    }
}

/// Keys on the realm create-game overlay: pick a ruleset, then the daily
/// reset hour, then type a name; Enter advances and finally creates. The
/// name step owns printable input, so `j`/`k` type rather than scroll.
/// The join overlay: pick a colour nobody there is wearing, or back out.
fn handle_realm_join_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.realm.join_cancel();
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.realm.join_confirm();
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.realm.join_move_selection(1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.realm.join_move_selection(-1);
        }
        ParsedInput::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.realm.join_move_selection(-1),
            MouseEventKind::ScrollDown => app.realm.join_move_selection(1),
            _ => {}
        },
        _ => {}
    }
}

fn handle_realm_draft_input(app: &mut App, event: ParsedInput) {
    use crate::app::lobby::realm::state::CreateStep;
    let step = app.realm.create_draft.as_ref().map(|draft| draft.step);
    let naming = step == Some(CreateStep::Name);
    match event {
        // Space flips a rule; Enter is reserved for "make the game".
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') if step == Some(CreateStep::Options) => {
            app.realm.draft_toggle_option();
        }
        ParsedInput::Byte(0x1B) => {
            app.realm.draft_back();
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.realm.draft_confirm();
        }
        _ if naming => match event {
            ParsedInput::Byte(0x7F | 0x08) => app.realm.draft_pop_name(),
            ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
                app.realm.draft_push_name(byte as char);
            }
            ParsedInput::Char(ch) => app.realm.draft_push_name(ch),
            _ => {}
        },
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.realm.draft_move_selection(1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.realm.draft_move_selection(-1);
        }
        // Left/right are only bound on the shape step, where j/k picks which
        // number and h/l changes it. Everywhere else the overlay is a list
        // and a sideways key means nothing.
        ParsedInput::Arrow(b'C')
        | ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
            if step == Some(CreateStep::MapShape) =>
        {
            app.realm.draft_adjust_shape(1);
        }
        ParsedInput::Arrow(b'D')
        | ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
            if step == Some(CreateStep::MapShape) =>
        {
            app.realm.draft_adjust_shape(-1);
        }
        _ => {}
    }
}

/// Open a realm's board from the Lobby, keeping the return screen the way
/// `activate_selection` does.
fn open_realm_board(app: &mut App, game_id: uuid::Uuid) {
    let return_screen = if app.screen == Screen::Realm {
        app.realm
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(Screen::Dashboard)
    } else {
        app.screen
    };
    app.realm.open_board(game_id, return_screen);
    app.show_lobby_modal = false;
    app.set_screen(Screen::Realm);
}
