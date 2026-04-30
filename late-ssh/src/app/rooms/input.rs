use crate::app::{
    common::primitives::Banner,
    input::{ParsedInput, sanitize_paste_markers},
    rooms::{
        blackjack::settings::{BlackjackTableSettings, PACE_OPTIONS, STAKE_OPTIONS},
        filter::RoomsFilter,
    },
    state::App,
};

const DISPLAY_NAME_MAX_LEN: usize = 48;
const DEFAULT_BLACKJACK_TABLE_NAME: &str = "Blackjack Table";
const SEARCH_QUERY_MAX_LEN: usize = 32;
const CREATE_FIELD_COUNT: usize = 3;
const CREATE_FIELD_NAME: usize = 0;
const CREATE_FIELD_PACE: usize = 1;
const CREATE_FIELD_STAKE: usize = 2;

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.rooms_active_room.is_some() && !app.rooms_add_form_open {
        match event {
            ParsedInput::Byte(byte) => return handle_active_room_key(app, *byte),
            ParsedInput::Char(ch) if ch.is_ascii() => {
                return handle_active_room_key(app, *ch as u8);
            }
            _ => {}
        }
    }

    if app.rooms_add_form_open {
        return handle_create_form_event(app, event);
    }

    if app.rooms_search_active {
        return handle_search_event(app, event);
    }

    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            handle_enter(app);
            true
        }
        ParsedInput::Byte(0x1B) => {
            handle_escape(app);
            true
        }
        ParsedInput::Char('/') => {
            enter_search(app);
            true
        }
        ParsedInput::Char('n') | ParsedInput::Char('N') => {
            open_create_form(app);
            true
        }
        ParsedInput::Char('j') => {
            move_selection(app, 1);
            true
        }
        ParsedInput::Char('k') => {
            move_selection(app, -1);
            true
        }
        ParsedInput::Char('h' | 'H') => {
            cycle_filter(app, false);
            true
        }
        ParsedInput::Char('l' | 'L') => {
            cycle_filter(app, true);
            true
        }
        _ => false,
    }
}

fn handle_create_form_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            submit_create_form(app);
            true
        }
        ParsedInput::Byte(0x1B) => {
            app.rooms_add_form_open = false;
            true
        }
        ParsedInput::Byte(b'\t') => {
            move_create_focus(app, 1);
            true
        }
        ParsedInput::BackTab => {
            move_create_focus(app, -1);
            true
        }
        ParsedInput::Arrow(b'A') => {
            move_create_focus(app, -1);
            true
        }
        ParsedInput::Arrow(b'B') => {
            move_create_focus(app, 1);
            true
        }
        ParsedInput::Arrow(b'D') => {
            adjust_create_selection(app, -1);
            true
        }
        ParsedInput::Arrow(b'C') => {
            adjust_create_selection(app, 1);
            true
        }
        ParsedInput::Char('a' | 'A') if app.rooms_create_focus_index != CREATE_FIELD_NAME => {
            adjust_create_selection(app, -1);
            true
        }
        ParsedInput::Char('d' | 'D') if app.rooms_create_focus_index != CREATE_FIELD_NAME => {
            adjust_create_selection(app, 1);
            true
        }
        ParsedInput::Byte(0x08 | 0x7F) if app.rooms_create_focus_index == CREATE_FIELD_NAME => {
            app.rooms_display_name_input.pop();
            true
        }
        ParsedInput::Byte(0x17) if app.rooms_create_focus_index == CREATE_FIELD_NAME => {
            app.rooms_display_name_input.clear();
            true
        }
        ParsedInput::Char(ch) if app.rooms_create_focus_index == CREATE_FIELD_NAME => {
            push_display_name_char(app, *ch);
            true
        }
        ParsedInput::Byte(byte) if app.rooms_create_focus_index == CREATE_FIELD_NAME => {
            if byte.is_ascii_graphic() || *byte == b' ' {
                push_display_name_char(app, *byte as char);
            }
            true
        }
        ParsedInput::Paste(bytes) if app.rooms_create_focus_index == CREATE_FIELD_NAME => {
            let pasted = String::from_utf8_lossy(bytes);
            for ch in sanitize_paste_markers(&pasted).chars() {
                push_display_name_char(app, ch);
            }
            true
        }
        _ => true,
    }
}

pub fn handle_key(app: &mut App, byte: u8) {
    if app.rooms_active_room.is_some() && !app.rooms_add_form_open {
        handle_active_room_key(app, byte);
        return;
    }

    match byte {
        b'\r' | b'\n' => handle_enter(app),
        0x1B => handle_escape(app),
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.rooms_add_form_open || app.rooms_active_room.is_some() || app.rooms_search_active {
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
        b'D' => {
            cycle_filter(app, false);
            true
        }
        b'C' => {
            cycle_filter(app, true);
            true
        }
        _ => false,
    }
}

fn handle_search_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => {
            app.rooms_search_active = false;
            clamp_selection(app);
            true
        }
        ParsedInput::Byte(0x1B) => {
            app.rooms_search_active = false;
            app.rooms_search_query.clear();
            clamp_selection(app);
            true
        }
        ParsedInput::Byte(0x08 | 0x7F) => {
            app.rooms_search_query.pop();
            clamp_selection(app);
            true
        }
        ParsedInput::Byte(0x17) => {
            app.rooms_search_query.clear();
            clamp_selection(app);
            true
        }
        ParsedInput::Char(ch) => {
            push_search_char(app, *ch);
            clamp_selection(app);
            true
        }
        ParsedInput::Byte(byte) => {
            if byte.is_ascii_graphic() || *byte == b' ' {
                push_search_char(app, *byte as char);
                clamp_selection(app);
            }
            true
        }
        ParsedInput::Paste(bytes) => {
            let pasted = String::from_utf8_lossy(bytes);
            for ch in sanitize_paste_markers(&pasted).chars() {
                push_search_char(app, ch);
            }
            clamp_selection(app);
            true
        }
        _ => true,
    }
}

fn handle_enter(app: &mut App) {
    if app.rooms_add_form_open {
        submit_create_form(app);
        return;
    }

    if visible_real_count(app) == 0 {
        return;
    }
    enter_selected_room(app);
}

fn submit_create_form(app: &mut App) {
    if !can_create_room(app.is_admin) {
        app.banner = Some(Banner::error(
            "Admin only: creating rooms is locked for now.",
        ));
        app.rooms_add_form_open = false;
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
        selected_blackjack_settings(app),
    );
    app.rooms_display_name_input.clear();
    app.rooms_add_form_open = false;

    app.banner = Some(Banner::success("Creating Blackjack table."));
}

fn open_create_form(app: &mut App) {
    if !can_create_room(app.is_admin) {
        app.banner = Some(Banner::error(
            "Admin only: creating rooms is locked for now.",
        ));
        return;
    }
    app.rooms_add_form_open = true;
    app.rooms_create_focus_index = CREATE_FIELD_NAME;
    if app.rooms_display_name_input.trim().is_empty() {
        app.rooms_display_name_input = DEFAULT_BLACKJACK_TABLE_NAME.to_string();
    }
}

fn handle_escape(app: &mut App) {
    if app.rooms_add_form_open {
        app.rooms_add_form_open = false;
        return;
    }
    if app.rooms_search_active {
        app.rooms_search_active = false;
        app.rooms_search_query.clear();
        clamp_selection(app);
        return;
    }
    if !app.rooms_search_query.is_empty() {
        app.rooms_search_query.clear();
        clamp_selection(app);
        return;
    }
    if app.rooms_filter != RoomsFilter::All {
        app.rooms_filter = RoomsFilter::All;
        clamp_selection(app);
        return;
    }
    app.rooms_active_room = None;
}

fn cycle_filter(app: &mut App, forward: bool) {
    app.rooms_filter = app.rooms_filter.cycle(forward);
    clamp_selection(app);
}

fn enter_search(app: &mut App) {
    app.rooms_search_active = true;
}

fn move_create_focus(app: &mut App, delta: isize) {
    app.rooms_create_focus_index =
        cycle_index(app.rooms_create_focus_index, CREATE_FIELD_COUNT, delta);
}

fn adjust_create_selection(app: &mut App, delta: isize) {
    match app.rooms_create_focus_index {
        CREATE_FIELD_PACE => {
            app.rooms_create_pace_index =
                cycle_index(app.rooms_create_pace_index, PACE_OPTIONS.len(), delta);
        }
        CREATE_FIELD_STAKE => {
            app.rooms_create_stake_index =
                cycle_index(app.rooms_create_stake_index, STAKE_OPTIONS.len(), delta);
        }
        _ => {}
    }
}

fn cycle_index(index: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
}

fn selected_blackjack_settings(app: &App) -> BlackjackTableSettings {
    BlackjackTableSettings {
        pace: PACE_OPTIONS
            .get(app.rooms_create_pace_index)
            .copied()
            .unwrap_or_default(),
        stake: STAKE_OPTIONS
            .get(app.rooms_create_stake_index)
            .copied()
            .unwrap_or(STAKE_OPTIONS[0]),
    }
    .normalized()
}

fn push_display_name_char(app: &mut App, ch: char) {
    if !is_input_char(ch) {
        return;
    }
    if app.rooms_display_name_input.chars().count() >= DISPLAY_NAME_MAX_LEN {
        return;
    }
    app.rooms_display_name_input.push(ch);
}

fn push_search_char(app: &mut App, ch: char) {
    if !is_input_char(ch) {
        return;
    }
    if app.rooms_search_query.chars().count() >= SEARCH_QUERY_MAX_LEN {
        return;
    }
    app.rooms_search_query.push(ch);
}

fn is_input_char(ch: char) -> bool {
    !ch.is_control() && ch != '\n' && ch != '\r'
}

fn move_selection(app: &mut App, delta: isize) {
    let count = visible_real_count(app);
    if count == 0 {
        app.rooms_selected_index = 0;
        return;
    }
    let max = count - 1;
    let next = app
        .rooms_selected_index
        .saturating_add_signed(delta)
        .min(max);
    app.rooms_selected_index = next;
}

fn clamp_selection(app: &mut App) {
    let count = visible_real_count(app);
    if count == 0 {
        app.rooms_selected_index = 0;
    } else {
        app.rooms_selected_index = app.rooms_selected_index.min(count - 1);
    }
}

fn visible_real_count(app: &App) -> usize {
    let q = app.rooms_search_query.trim().to_lowercase();
    app.rooms_snapshot
        .rooms
        .iter()
        .filter(|room| app.rooms_filter.matches_real(room.game_kind))
        .filter(|room| q.is_empty() || room.display_name.to_lowercase().contains(&q))
        .count()
}

fn visible_real_room_at(app: &App, index: usize) -> Option<crate::app::rooms::svc::RoomListItem> {
    let q = app.rooms_search_query.trim().to_lowercase();
    app.rooms_snapshot
        .rooms
        .iter()
        .filter(|room| app.rooms_filter.matches_real(room.game_kind))
        .filter(|room| q.is_empty() || room.display_name.to_lowercase().contains(&q))
        .nth(index)
        .cloned()
}

fn enter_selected_room(app: &mut App) {
    if !can_enter_room(app.is_admin, app.is_mod) {
        app.banner = Some(Banner::error(
            "Admin or mod only: rooms are locked for now.",
        ));
        return;
    }

    if let Some(room) = visible_real_room_at(app, app.rooms_selected_index) {
        app.chat.join_game_room_chat(room.chat_room_id);
        app.chat.request_room_tail(room.chat_room_id);
        if matches!(room.game_kind, crate::app::rooms::svc::GameKind::Blackjack) {
            app.rooms_service.touch_room_task(room.id);
            let svc = app
                .blackjack_table_manager
                .get_or_create(room.id, room.blackjack_settings.clone());
            app.blackjack_state =
                crate::app::rooms::blackjack::state::State::new(svc, app.user_id, app.chip_balance);
        }
        app.rooms_active_room = Some(room);
        app.rooms_add_form_open = false;
    }
}

fn handle_active_room_key(app: &mut App, byte: u8) -> bool {
    let Some(room) = app.rooms_active_room.as_ref() else {
        return false;
    };
    let game_kind = room.game_kind;
    let chat_room_id = room.chat_room_id;

    if matches!(byte, b'i' | b'I') {
        app.chat.start_composing_in_room(chat_room_id);
        return true;
    }

    match game_kind {
        crate::app::rooms::svc::GameKind::Blackjack => {
            let byte = if matches!(byte, b'q' | b'Q') {
                0x1B
            } else {
                byte
            };
            match crate::app::rooms::blackjack::input::handle_key(&mut app.blackjack_state, byte) {
                crate::app::rooms::blackjack::input::InputAction::Ignored => false,
                crate::app::rooms::blackjack::input::InputAction::Handled => true,
                crate::app::rooms::blackjack::input::InputAction::Leave => {
                    app.rooms_active_room = None;
                    true
                }
            }
        }
    }
}

fn can_create_room(is_admin: bool) -> bool {
    is_admin
}

fn can_enter_room(is_admin: bool, is_mod: bool) -> bool {
    is_admin || is_mod
}

#[cfg(test)]
mod tests {
    use super::{can_create_room, can_enter_room};

    #[test]
    fn room_creation_stays_admin_only() {
        assert!(can_create_room(true));
        assert!(!can_create_room(false));
    }

    #[test]
    fn room_entry_allows_admins_and_mods() {
        assert!(can_enter_room(true, false));
        assert!(can_enter_room(false, true));
        assert!(can_enter_room(true, true));
        assert!(!can_enter_room(false, false));
    }
}
