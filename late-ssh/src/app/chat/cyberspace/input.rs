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
    BODY_MAX_CHARS, ComposeField, LinkField, Modal, TITLE_MAX_CHARS, TOPICS_MAX_CHARS, View,
};

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
    let action = match &mut app.chat.cyberspace.modal {
        None => ModalAction::None,
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

pub(crate) fn handle_modal_escape(app: &mut App) {
    app.chat.cyberspace.close_modal();
}

fn modal_busy(modal: &Modal) -> bool {
    match modal {
        Modal::Link(link) => link.busy,
        Modal::Compose(compose) => compose.busy,
        Modal::Reply(reply) => reply.busy,
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
