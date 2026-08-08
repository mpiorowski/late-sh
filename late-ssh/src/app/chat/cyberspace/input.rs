use crate::app::{
    common::{
        composer::set_themed_textarea_cursor_visible,
        primitives::Banner,
        textarea_input::{EditOutcome, handle_multiline_edit, handle_single_line_edit},
    },
    input::ParsedInput,
    state::App,
};

use super::state::{
    BODY_MAX_CHARS, CIRC_MESSAGE_MAX_CHARS, ComposeField, LinkField, Modal, TITLE_MAX_CHARS,
    TOPICS_MAX_CHARS, View,
};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    let in_room = app.chat.cyberspace.open_room_slug().is_some();
    match (key, in_room) {
        (b'A', true) => {
            app.chat.cyberspace.room_scroll(-1);
            true
        }
        (b'B', true) => {
            app.chat.cyberspace.room_scroll(1);
            true
        }
        (b'A', false) => {
            app.chat.cyberspace.move_selection(-1);
            true
        }
        (b'B', false) => {
            app.chat.cyberspace.move_selection(1);
            true
        }
        _ => false,
    }
}

/// Keys inside an open chat room. The room is its own surface (its own rail
/// slot), so it does not share the pane's view keys.
pub fn handle_room_byte(app: &mut App, byte: u8) -> bool {
    let state = &mut app.chat.cyberspace;
    match byte {
        b'j' | b'J' => {
            state.room_scroll(1);
            true
        }
        b'k' | b'K' => {
            state.room_scroll(-1);
            true
        }
        b'g' | b'G' => {
            state.room_to_bottom();
            true
        }
        b'i' | b'I' | b'\r' | b'\n' => {
            state.start_room_composer();
            true
        }
        _ => false,
    }
}

/// Keystrokes while the room composer is open. It owns every one of them, the
/// same way an open modal does, so a message can contain a space or an `h`
/// without the rail stealing it. Reached from `app::input`'s modal chain,
/// which runs before any chat routing.
pub fn handle_room_composer_input(app: &mut App, event: ParsedInput) {
    let Some(composer) = app.chat.cyberspace.room_composer_mut() else {
        return;
    };
    // Single line: one keypress sends, matching the one-row composer slot it
    // draws in and their one-message-per-send API.
    match handle_single_line_edit(composer, &event, CIRC_MESSAGE_MAX_CHARS) {
        EditOutcome::Submit => {
            if let Some(banner) = app.chat.cyberspace.submit_room_composer() {
                app.banner = Some(banner);
            }
        }
        EditOutcome::Cancel => {
            app.chat.cyberspace.cancel_room_composer();
        }
        EditOutcome::Handled => app.chat.cyberspace.note_composer_activity(),
        EditOutcome::Ignored => {}
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    let state = &mut app.chat.cyberspace;
    if !state.is_linked() {
        // The pane is the pitch + login funnel until an account is linked.
        // Unhandled bytes fall through so global keys (quit, jump) keep working.
        if matches!(byte, b'\r' | b'\n') {
            state.open_link_modal();
            return true;
        }
        return false;
    }
    match byte {
        b'j' | b'J' => {
            state.move_selection(1);
            true
        }
        b'k' | b'K' => {
            state.move_selection(-1);
            true
        }
        b'g' | b'G' => {
            state.go_to_top();
            true
        }
        b'\r' | b'\n' => {
            match state.view {
                View::Feed => state.open_selected_thread(),
                View::Notifications => {
                    if let Some(banner) = state.open_selected_notification() {
                        app.banner = Some(banner);
                    }
                }
                View::Thread => {}
            }
            true
        }
        b'c' | b'C' => {
            if let Some(banner) = state.open_rooms_modal() {
                app.banner = Some(banner);
            }
            true
        }
        b'r' | b'R' => {
            match state.view {
                View::Feed => {
                    state.refresh();
                    app.banner = Some(Banner::success("Refreshing cyberspace..."));
                }
                View::Thread => state.open_reply_modal(),
                View::Notifications => state.open_notifications(),
            }
            true
        }
        b'p' | b'P' => {
            if let Some(banner) = state.open_compose_modal() {
                app.banner = Some(banner);
            }
            true
        }
        b'n' | b'N' => {
            state.open_notifications();
            true
        }
        // Esc never arrives here: it is routed through `dispatch_escape`,
        // which has its own arm for the pane.
        b'b' | b'B' => {
            if state.view != View::Feed {
                state.back_to_feed();
            }
            true
        }
        _ => false,
    }
}

/// What a modal keystroke resolved to, decided while the modal is borrowed
/// and acted on after the borrow ends.
enum ModalAction {
    None,
    Submit,
    Escape,
}

pub(crate) fn handle_modal_input(app: &mut App, event: ParsedInput) {
    if matches!(event, ParsedInput::Byte(0x1B)) {
        handle_modal_escape(app);
        return;
    }
    // The room picker is a list, not a form: it moves and toggles rather than
    // editing text, so it is resolved before the text-field modals.
    if matches!(app.chat.cyberspace.modal, Some(Modal::Rooms(_))) {
        handle_rooms_modal_input(app, event);
        return;
    }
    let action = match &mut app.chat.cyberspace.modal {
        // Handled above; the arm keeps the match exhaustive over the roster.
        None | Some(Modal::Rooms(_)) => ModalAction::None,
        // While a submit is in flight the draft is locked; Esc above still works.
        Some(modal) if modal_busy(modal) => ModalAction::None,
        Some(Modal::Link(link)) => match event {
            ParsedInput::Byte(b'\t')
            | ParsedInput::Arrow(b'B')
            | ParsedInput::BackTab
            | ParsedInput::Arrow(b'A') => {
                link.focus = match link.focus {
                    LinkField::Email => LinkField::Password,
                    LinkField::Password => LinkField::Email,
                };
                set_themed_textarea_cursor_visible(&mut link.email, link.focus == LinkField::Email);
                set_themed_textarea_cursor_visible(
                    &mut link.password,
                    link.focus == LinkField::Password,
                );
                ModalAction::None
            }
            event => {
                let field = match link.focus {
                    LinkField::Email => &mut link.email,
                    LinkField::Password => &mut link.password,
                };
                match handle_single_line_edit(field, &event, 256) {
                    EditOutcome::Submit => ModalAction::Submit,
                    EditOutcome::Cancel => ModalAction::Escape,
                    EditOutcome::Handled | EditOutcome::Ignored => ModalAction::None,
                }
            }
        },
        Some(Modal::Compose(compose)) => match event {
            ParsedInput::Byte(b'\t') => {
                compose.focus = next_compose_field(compose.focus, 1);
                sync_compose_focus(compose);
                ModalAction::None
            }
            ParsedInput::BackTab => {
                compose.focus = next_compose_field(compose.focus, -1);
                sync_compose_focus(compose);
                ModalAction::None
            }
            event => {
                let outcome = match compose.focus {
                    ComposeField::Title => {
                        handle_single_line_edit(&mut compose.title, &event, TITLE_MAX_CHARS)
                    }
                    ComposeField::Topics => {
                        handle_single_line_edit(&mut compose.topics, &event, TOPICS_MAX_CHARS)
                    }
                    ComposeField::Body => {
                        handle_multiline_edit(&mut compose.body, &event, BODY_MAX_CHARS)
                    }
                };
                match outcome {
                    // Enter in the metadata fields walks down to the body;
                    // only the body's Enter publishes.
                    EditOutcome::Submit if compose.focus != ComposeField::Body => {
                        compose.focus = next_compose_field(compose.focus, 1);
                        sync_compose_focus(compose);
                        ModalAction::None
                    }
                    EditOutcome::Submit => ModalAction::Submit,
                    EditOutcome::Cancel => ModalAction::Escape,
                    EditOutcome::Handled | EditOutcome::Ignored => ModalAction::None,
                }
            }
        },
        Some(Modal::Reply(reply)) => {
            match handle_multiline_edit(&mut reply.body, &event, BODY_MAX_CHARS) {
                EditOutcome::Submit => ModalAction::Submit,
                EditOutcome::Cancel => ModalAction::Escape,
                EditOutcome::Handled | EditOutcome::Ignored => ModalAction::None,
            }
        }
    };
    match action {
        ModalAction::None => {}
        ModalAction::Submit => app.chat.cyberspace.submit_modal(),
        ModalAction::Escape => handle_modal_escape(app),
    }
}

/// The room picker: move the highlight, toggle a room onto the rail, close.
/// Nothing here opens a room, because the rail entry is how a room is entered.
fn handle_rooms_modal_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'j' | b'J') | ParsedInput::Arrow(b'B') => {
            app.chat.cyberspace.move_rooms_modal_selection(1);
        }
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::Arrow(b'A') => {
            app.chat.cyberspace.move_rooms_modal_selection(-1);
        }
        ParsedInput::Byte(b'\r' | b'\n' | b' ') => {
            if let Some(banner) = app.chat.cyberspace.toggle_selected_room() {
                app.banner = Some(banner);
            }
        }
        // Everything else is swallowed: an open modal owns the keyboard.
        _ => {}
    }
}

pub(crate) fn handle_modal_escape(app: &mut App) {
    app.chat.cyberspace.close_modal();
}

fn modal_busy(modal: &Modal) -> bool {
    match modal {
        Modal::Link(link) => link.busy,
        Modal::Compose(compose) => compose.busy,
        Modal::Reply(reply) => reply.busy,
        // The picker is never busy in this sense: it holds no draft to
        // protect, and toggling stays usable while its roster loads.
        Modal::Rooms(_) => false,
    }
}

fn next_compose_field(current: ComposeField, delta: isize) -> ComposeField {
    const ORDER: [ComposeField; 3] = [
        ComposeField::Title,
        ComposeField::Topics,
        ComposeField::Body,
    ];
    let index = ORDER
        .iter()
        .position(|field| *field == current)
        .expect("compose field is in its own order") as isize;
    ORDER[(index + delta).rem_euclid(ORDER.len() as isize) as usize]
}

fn sync_compose_focus(compose: &mut super::state::ComposeModal) {
    set_themed_textarea_cursor_visible(&mut compose.title, compose.focus == ComposeField::Title);
    set_themed_textarea_cursor_visible(&mut compose.topics, compose.focus == ComposeField::Topics);
    set_themed_textarea_cursor_visible(&mut compose.body, compose.focus == ComposeField::Body);
}
