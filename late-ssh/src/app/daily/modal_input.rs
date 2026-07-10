use crate::app::common::primitives::Screen;
use crate::app::daily::state::DailyModalEntry;
use crate::app::input::ParsedInput;
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
            app.daily.move_selection(1);
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.daily.move_selection(-1);
        }
        ParsedInput::Byte(b'\r' | b'\n' | b' ') | ParsedInput::Char(' ') => {
            activate_selection(app);
        }
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => {
            app.daily.begin_challenge_draft(false);
        }
        ParsedInput::Byte(b'C') | ParsedInput::Char('C') => {
            app.daily.begin_challenge_draft(true);
        }
        ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
            let own = match app.daily.selected_entry() {
                Some(DailyModalEntry::Challenge(challenge))
                    if challenge.challenger_id == app.daily.user_id() =>
                {
                    Some(challenge.id)
                }
                _ => None,
            };
            if let Some(match_id) = own {
                app.daily.cancel_challenge(match_id);
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    if app.daily.challenge_draft.take().is_some() {
        return;
    }
    if app.daily.confirm_claim.take().is_some() {
        return;
    }
    // Everything visible in the modal has been seen; don't glow for it.
    app.daily.mark_lobby_seen();
    app.show_daily_modal = false;
}

/// Enter on a match opens its board; Enter on someone else's challenge asks
/// for confirmation, then claims.
fn activate_selection(app: &mut App) {
    enum Action {
        OpenBoard(crate::app::daily::svc::DailyMatchItem),
        ConfirmClaim(uuid::Uuid),
        Claim(uuid::Uuid),
    }
    let action = match app.daily.selected_entry() {
        Some(DailyModalEntry::Match(item)) => Some(Action::OpenBoard(item.clone())),
        Some(DailyModalEntry::Challenge(challenge)) => {
            if challenge.challenger_id == app.daily.user_id() {
                None
            } else if app.daily.confirm_claim == Some(challenge.id) {
                Some(Action::Claim(challenge.id))
            } else {
                Some(Action::ConfirmClaim(challenge.id))
            }
        }
        None => None,
    };
    match action {
        Some(Action::OpenBoard(item)) => {
            // Switching matches while a board is already open keeps the
            // original return screen, so Esc never lands on a dead board.
            let return_screen = if app.screen == Screen::DailyMatch {
                app.daily
                    .board
                    .as_ref()
                    .map(|board| board.return_screen)
                    .unwrap_or(Screen::Dashboard)
            } else {
                app.screen
            };
            app.daily.open_board(&item, return_screen);
            app.show_daily_modal = false;
            app.set_screen(Screen::DailyMatch);
        }
        Some(Action::ConfirmClaim(match_id)) => {
            app.daily.confirm_claim = Some(match_id);
        }
        Some(Action::Claim(match_id)) => {
            app.daily.claim_challenge(match_id);
        }
        None => {}
    }
}

/// Keys while composing a challenge: Tab / ←/→ cycle the game, Enter posts,
/// Esc cancels, and everything printable edits the directed username buffer.
fn handle_draft_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => {
            app.daily.challenge_draft = None;
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.daily.submit_challenge_draft();
        }
        ParsedInput::Byte(b'\t') | ParsedInput::Arrow(b'C') => {
            app.daily.cycle_draft_game(true);
        }
        ParsedInput::Arrow(b'D') => {
            app.daily.cycle_draft_game(false);
        }
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
    }
}

fn draft_username_buffer(app: &mut App) -> Option<&mut String> {
    app.daily
        .challenge_draft
        .as_mut()
        .and_then(|draft| draft.target.as_mut())
}

fn push_prompt_char(app: &mut App, ch: char) {
    const MAX_USERNAME_PROMPT: usize = 32;
    if let Some(buffer) = draft_username_buffer(app)
        && buffer.chars().count() < MAX_USERNAME_PROMPT
    {
        buffer.push(ch);
    }
}
