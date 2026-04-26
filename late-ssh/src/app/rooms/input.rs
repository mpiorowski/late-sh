use crate::app::{
    common::primitives::Banner,
    input::{ParsedInput, sanitize_paste_markers},
    state::App,
};

const DISPLAY_NAME_MAX_LEN: usize = 48;
const DEFAULT_BLACKJACK_TABLE_NAME: &str = "Blackjack Table";

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            handle_enter(app);
            true
        }
        ParsedInput::Byte(0x1B) => {
            handle_escape(app);
            true
        }
        ParsedInput::Byte(0x08 | 0x7F) if app.rooms_add_form_open => {
            app.rooms_display_name_input.pop();
            true
        }
        ParsedInput::Byte(0x17) if app.rooms_add_form_open => {
            app.rooms_display_name_input.clear();
            true
        }
        ParsedInput::Char(ch) if app.rooms_add_form_open => {
            push_display_name_char(app, *ch);
            true
        }
        ParsedInput::Byte(byte) if app.rooms_add_form_open => {
            if byte.is_ascii_graphic() || *byte == b' ' {
                push_display_name_char(app, *byte as char);
            }
            true
        }
        ParsedInput::Paste(bytes) if app.rooms_add_form_open => {
            let pasted = String::from_utf8_lossy(bytes);
            for ch in sanitize_paste_markers(&pasted).chars() {
                push_display_name_char(app, ch);
            }
            true
        }
        _ => false,
    }
}

pub fn handle_key(app: &mut App, byte: u8) {
    match byte {
        b'\r' | b'\n' => handle_enter(app),
        0x1B => handle_escape(app),
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.rooms_add_form_open || app.rooms_active_room.is_some() {
        return false;
    }

    match key {
        b'A' => {
            move_selection(app, -1);
            true
        }
        b'B' => {
            move_selection(app, 1);
            true
        }
        _ => false,
    }
}

fn handle_enter(app: &mut App) {
    if !app.rooms_add_form_open {
        if app.rooms_selected_index > 0 {
            enter_selected_room(app);
            return;
        }
        app.rooms_add_form_open = true;
        if app.rooms_display_name_input.trim().is_empty() {
            app.rooms_display_name_input = DEFAULT_BLACKJACK_TABLE_NAME.to_string();
        }
        return;
    }

    let display_name = app.rooms_display_name_input.trim().to_string();
    if display_name.is_empty() {
        app.banner = Some(Banner::error("Table name is required."));
        return;
    }

    app.rooms_service.create_game_room_task(
        app.user_id,
        crate::app::rooms::svc::GameKind::Blackjack,
        display_name,
    );
    app.rooms_display_name_input.clear();
    app.rooms_add_form_open = false;

    app.banner = Some(Banner::success("Creating Blackjack table."));
}

fn handle_escape(app: &mut App) {
    if app.rooms_add_form_open {
        app.rooms_add_form_open = false;
        return;
    }
    app.rooms_active_room = None;
}

fn push_display_name_char(app: &mut App, ch: char) {
    if !is_display_name_char(ch) {
        return;
    }
    if app.rooms_display_name_input.chars().count() >= DISPLAY_NAME_MAX_LEN {
        return;
    }
    app.rooms_display_name_input.push(ch);
}

fn is_display_name_char(ch: char) -> bool {
    !ch.is_control() && ch != '\n' && ch != '\r'
}

fn move_selection(app: &mut App, delta: isize) {
    let max_index = app.rooms_snapshot.rooms.len();
    let next = app
        .rooms_selected_index
        .saturating_add_signed(delta)
        .min(max_index);
    app.rooms_selected_index = next;
}

fn enter_selected_room(app: &mut App) {
    let room_index = app.rooms_selected_index.saturating_sub(1);
    if let Some(room) = app.rooms_snapshot.rooms.get(room_index).cloned() {
        app.rooms_active_room = Some(room);
        app.rooms_add_form_open = false;
    }
}
