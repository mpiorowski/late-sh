use crate::app::common::primitives::Screen;
use crate::app::common::textarea_input::{EditOutcome, handle_freeform_edit};
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::state::ScratchpadState;

/// The pending-invite prompt owns all input while it's up, same shape as
/// `quit_confirm::input::handle_input` ("a lightweight Y/N prompt that
/// pre-empts whatever screen is showing").
pub(crate) fn handle_invite_prompt(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r')
        | ParsedInput::Char('y' | 'Y')
        | ParsedInput::Byte(b'y' | b'Y') => {
            accept_pending_invite(app);
        }
        ParsedInput::Byte(0x1B) | ParsedInput::Char('n' | 'N') | ParsedInput::Byte(b'n' | b'N') => {
            app.pair_invite_pending = None;
        }
        _ => {}
    }
}

fn accept_pending_invite(app: &mut App) {
    let Some(invite) = app.pair_invite_pending.take() else {
        return;
    };
    let Some(registry) = app.scratchpad_registry.clone() else {
        return;
    };
    let partner_id = invite.from_user_id;
    let partner_username = invite.from_username.clone();
    let shared = registry.accept(app.user_id, app.username.clone(), invite);
    app.scratchpad = Some(ScratchpadState::new(
        registry,
        shared,
        app.user_id,
        partner_id,
        partner_username,
    ));
    app.set_screen(Screen::Scratchpad);
}

/// Full-screen dispatch for the paired editor: forward everything to the
/// shared `TextArea`, same shape as the daily board / house table in
/// `handle_dedicated_screen_input`.
pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(state) = app.scratchpad.as_mut() else {
        return false;
    };
    let max_chars = state.max_chars();
    match handle_freeform_edit(&mut state.editor, event, max_chars) {
        EditOutcome::Handled => {
            state.publish();
            true
        }
        EditOutcome::Cancel => {
            app.set_screen(Screen::Dashboard);
            true
        }
        EditOutcome::Submit => true,
        EditOutcome::Ignored => false,
    }
}
