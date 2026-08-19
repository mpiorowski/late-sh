use crate::app::{
    chat::state::{CyberspaceCommand, parse_cyberspace_command},
    common::{
        composer::set_themed_textarea_cursor_visible,
        primitives::Banner,
        textarea_input::{EditOutcome, handle_multiline_edit, handle_single_line_edit},
    },
    input::ParsedInput,
    state::App,
};

use super::state::{
    BODY_MAX_CHARS, CIRC_MESSAGE_MAX_CHARS, ComposeField, LinkField, Modal, NotificationTarget,
    TITLE_MAX_CHARS, TOPICS_MAX_CHARS, View,
};

/// How far PageUp/PageDown move a room's conversation. A fixed jump rather
/// than a viewport height: only the renderer knows the height, and it is not
/// worth threading back for a scroll step.
const ROOM_PAGE_ROWS: isize = 10;

/// Arrows in the pane, or in a room being read: a room scrolls its
/// conversation where the pane moves its selection. Arrows inside a focused
/// composer never reach here, since `app::input` routes every event there
/// first.
pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    let in_room = app.chat.cyberspace.open_room_name().is_some();
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

/// Keys inside an open chat room while its composer is not focused. The room
/// is its own surface (its own rail slot), so it does not share the pane's
/// view keys, and unhandled bytes fall through so the global keys keep
/// working. Returning to reading rather than typing on entry is what keeps
/// them reachable at all.
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

/// Keystrokes while the room composer is focused. It owns every one of them,
/// the same way an open modal does, so a message can contain a space or an
/// `h` without the rail stealing it. Reached from `app::input`'s modal chain,
/// which runs before any chat routing. Scrolling rides the arrows and the
/// page keys here, since the letters are text while the composer has focus.
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
        // Only with nothing typed: while the row holds text, End is an
        // editing key (Home's mirror), and falls through to the edit below.
        ParsedInput::End => {
            if app.chat.cyberspace.room_composer_text().is_empty() {
                app.chat.cyberspace.room_to_bottom();
                return;
            }
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
            // A command of ours never reaches their API as a message. Their
            // own commands (`/me`, `/dice`, `/mute`, ...) expand server-side,
            // so anything we do not recognise is sent exactly as typed.
            match parse_cyberspace_command(&app.chat.cyberspace.room_composer_text()) {
                Some(command) => {
                    app.chat.cyberspace.clear_room_composer();
                    if let Some(banner) = handle_room_command(app, command) {
                        app.banner = Some(banner);
                    }
                }
                None => {
                    let keep_open = app.profile_state.profile().keep_composer_focused;
                    if let Some(banner) = app.chat.cyberspace.submit_room_composer(keep_open) {
                        app.banner = Some(banner);
                    }
                }
            }
        }
        // Esc: `app::input`'s escape chain owns both steps out of a room, and
        // it asks the composer first, so this arm never has to.
        EditOutcome::Cancel => {}
        EditOutcome::Handled => app.chat.cyberspace.note_composer_activity(),
        EditOutcome::Ignored => {}
    }
}

/// A `/cs` command typed inside one of their rooms, answered rather than
/// posted, because a command of ours has no business landing in their chat
/// as a message. The two pickers open *over* the room; `/cs mail @user`
/// deliberately moves the user into the conversation it starts, since a
/// conversation started by name is one they asked to write in. The rest
/// stay with the main chat composer.
fn handle_room_command(app: &mut App, command: CyberspaceCommand) -> Option<Banner> {
    match command {
        CyberspaceCommand::Chat => app.chat.cyberspace.open_rooms_modal(),
        CyberspaceCommand::Mail => app.chat.cyberspace.open_cmail_modal(),
        CyberspaceCommand::MailTo(username) => app.chat.cyberspace.start_cmail(username),
        CyberspaceCommand::Open
        | CyberspaceCommand::Post
        | CyberspaceCommand::Link
        | CyberspaceCommand::Unlink
        | CyberspaceCommand::Invalid => Some(Banner::error(
            "In a cyberspace room: /cs chat, /cs mail, /cs mail @user.",
        )),
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
                View::Notifications => match state.selected_notification_target() {
                    NotificationTarget::Entry(post_id) => state.open_notification_entry(post_id),
                    // A chat mention names a room, so Enter walks into it
                    // rather than opening an entry that does not exist.
                    NotificationTarget::ChatRoom(slug) => jump_to_chat_room(app, slug),
                    NotificationTarget::Nothing => {
                        app.banner = Some(Banner::error("That notification has nothing to open."));
                    }
                },
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
        // Their entries live on the web at `/{username}/{slug}`, and sharing
        // one is the reason to leave the pane with something in hand. Same
        // key and same banner as every other copy in the app.
        b'c' | b'C' => {
            let link = state.selected_entry_link();
            match link {
                Some(link) => {
                    app.pending_clipboard = Some(link);
                    app.banner = Some(Banner::success("Link copied!"));
                }
                None => app.banner = Some(Banner::error("That entry has no link to copy.")),
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

/// Enter on a chat mention: walk into the room it happened in. Their payload
/// carries no message id and no message timestamp, so the room is as far as a
/// jump can go, and a fresh page of history is what puts the mention on screen.
/// A room that is not on the rail is pinned first, because the rail entry is
/// how a room is entered and the chat tick leaves any open room the pinned
/// list cannot name.
fn jump_to_chat_room(app: &mut App, slug: String) {
    if app.chat.cyberspace.pin_room(slug.clone()) {
        app.banner = Some(Banner::success(&format!("Added #{slug} to your rail.")));
    }
    let Some(index) = app
        .chat
        .cyberspace
        .pinned_rooms()
        .iter()
        .position(|pinned| *pinned == slug)
    else {
        return;
    };
    app.chat.select_cyberspace_room(index);
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
