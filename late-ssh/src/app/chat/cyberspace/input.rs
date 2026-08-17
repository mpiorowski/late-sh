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

/// How far PageUp/PageDown move a room's conversation. A fixed jump rather
/// than a viewport height: only the renderer knows the height, and it is not
/// worth threading back for a scroll step.
const ROOM_PAGE_ROWS: isize = 10;

/// Arrows in the pane. A room's arrows never reach here: its composer is
/// always open, so `app::input` routes every event there first.
pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.cyberspace.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.cyberspace.move_selection(1);
            true
        }
        _ => false,
    }
}

/// Keystrokes inside an open chat room. The composer is always open, the way
/// it is in every other room in the rail, so it owns every one of them and a
/// message can contain a space or an `h` without the rail stealing it.
/// Reached from `app::input`'s modal chain, which runs before any chat
/// routing. Scrolling moves to the arrows, since the letters are text now.
pub fn handle_room_composer_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Arrow(b'A') => {
            app.chat.cyberspace.room_scroll(-1);
            return;
        }
        ParsedInput::Arrow(b'B') => {
            app.chat.cyberspace.room_scroll(1);
            return;
        }
        ParsedInput::PageUp => {
            app.chat.cyberspace.room_scroll(-ROOM_PAGE_ROWS);
            return;
        }
        ParsedInput::PageDown => {
            app.chat.cyberspace.room_scroll(ROOM_PAGE_ROWS);
            return;
        }
        // Back to the live bottom, however far back the user scrolled.
        ParsedInput::End => {
            app.chat.cyberspace.room_to_bottom();
            return;
        }
        _ => {}
    }
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
        // Esc: `app::input`'s escape chain owns leaving the room, and it asks
        // the composer first, so this arm never has to.
        EditOutcome::Cancel => {}
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
        b'r' | b'R' => {
            match state.view {
                View::Feed => {
                    state.refresh();
                    app.banner = Some(Banner::success("Refreshing cyberspace..."));
                }
                View::Thread => state.open_reply_modal(),
                View::Notifications => state.refresh_notifications(),
            }
            true
        }
        b'p' | b'P' => {
            if let Some(banner) = state.open_compose_modal() {
                app.banner = Some(banner);
            }
            true
        }
        // `n` moves the rail rather than swapping the view underneath it:
        // notifications are their own row, and the highlight has to follow.
        b'n' | b'N' => {
            app.chat.select_cyberspace_notifications();
            true
        }
        // Esc never arrives here: it is routed through `dispatch_escape`,
        // which has its own arm for the pane. `b` with nothing open over the
        // row is the way back off the notifications row, mirroring `n`.
        b'b' | b'B' => {
            if !state.escape_to_root() {
                app.chat.select_cyberspace();
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
    // The pickers are lists, not forms: they move and toggle rather than
    // editing text, so they are resolved before the text-field modals.
    if matches!(
        app.chat.cyberspace.modal,
        Some(Modal::Rooms(_) | Modal::Cmail(_))
    ) {
        handle_picker_modal_input(app, event);
        return;
    }
    let action = match &mut app.chat.cyberspace.modal {
        // Handled above; the arm keeps the match exhaustive over the roster.
        None | Some(Modal::Rooms(_) | Modal::Cmail(_)) => ModalAction::None,
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

/// The room and c-mail pickers: move the highlight, toggle the row onto the
/// rail, close. Nothing here opens anything, because the rail entry is how a
/// room or a conversation is entered.
fn handle_picker_modal_input(app: &mut App, event: ParsedInput) {
    let is_cmail = matches!(app.chat.cyberspace.modal, Some(Modal::Cmail(_)));
    match event {
        ParsedInput::Byte(b'j' | b'J') | ParsedInput::Arrow(b'B') => match is_cmail {
            true => app.chat.cyberspace.move_cmail_modal_selection(1),
            false => app.chat.cyberspace.move_rooms_modal_selection(1),
        },
        ParsedInput::Byte(b'k' | b'K') | ParsedInput::Arrow(b'A') => match is_cmail {
            true => app.chat.cyberspace.move_cmail_modal_selection(-1),
            false => app.chat.cyberspace.move_rooms_modal_selection(-1),
        },
        ParsedInput::Byte(b'\r' | b'\n' | b' ') => {
            let banner = match is_cmail {
                true => app.chat.cyberspace.toggle_selected_cmail(),
                false => app.chat.cyberspace.toggle_selected_room(),
            };
            if let Some(banner) = banner {
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
        // The pickers are never busy in this sense: they hold no draft to
        // protect, and toggling stays usable while the list loads.
        Modal::Rooms(_) | Modal::Cmail(_) => false,
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
