use crate::app::state::DashboardGameToggleTarget;
use crate::app::{
    common::primitives::{Banner, Screen},
    input::{MouseEventKind, ParsedInput, sanitize_paste_markers},
    rooms::{
        backend::{CreateModalAction, CreateRoomFlow, InputAction},
        filter::RoomsFilter,
    },
    state::App,
};

const SEARCH_QUERY_MAX_LEN: usize = 32;

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.rooms_active_room.is_some() && app.rooms_create_flow.is_none() {
        match event {
            ParsedInput::Byte(byte) => return handle_active_room_key(app, *byte),
            ParsedInput::Char(ch) if ch.is_ascii() => {
                return handle_active_room_key(app, *ch as u8);
            }
            ParsedInput::Arrow(key) => return handle_active_room_arrow(app, *key),
            ParsedInput::PageUp => {
                return handle_active_room_scroll(app, active_room_page_step(app));
            }
            ParsedInput::PageDown => {
                return handle_active_room_scroll(app, -active_room_page_step(app));
            }
            ParsedInput::End => return handle_active_room_scroll(app, isize::MIN),
            ParsedInput::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => return handle_active_room_scroll(app, 1),
                MouseEventKind::ScrollDown => return handle_active_room_scroll(app, -1),
                _ => {}
            },
            _ => {}
        }
    }

    if app.rooms_create_flow.is_some() {
        return handle_create_flow_event(app, event);
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
            open_create_picker(app);
            true
        }
        ParsedInput::Char('d') | ParsedInput::Char('D') => {
            delete_selected_room(app);
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

pub fn handle_key(app: &mut App, byte: u8) {
    if app.rooms_active_room.is_some() && app.rooms_create_flow.is_none() {
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
    if app.rooms_create_flow.is_some() || app.rooms_active_room.is_some() || app.rooms_search_active
    {
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
    if app.rooms_create_flow.is_some() {
        return;
    }

    if visible_real_count(app) == 0 {
        return;
    }
    enter_selected_room(app);
}

fn handle_create_flow_event(app: &mut App, event: &ParsedInput) -> bool {
    if let Some(CreateRoomFlow::Picker { kind_index }) = app.rooms_create_flow.as_ref() {
        return handle_create_picker_event(app, *kind_index, event);
    }

    let Some((kind, action)) = app.rooms_create_flow.as_mut().and_then(|flow| match flow {
        CreateRoomFlow::Game { kind, modal } => Some((*kind, modal.handle_event(event))),
        CreateRoomFlow::Picker { .. } => None,
    }) else {
        return false;
    };

    match action {
        CreateModalAction::Continue => true,
        CreateModalAction::Cancel => {
            app.rooms_create_flow = None;
            true
        }
        CreateModalAction::Submit {
            display_name,
            settings,
        } => {
            submit_create_modal(app, kind, display_name, settings);
            true
        }
    }
}

fn handle_create_picker_event(app: &mut App, kind_index: usize, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(0x1B) => {
            app.rooms_create_flow = None;
            true
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            open_selected_create_modal(app, kind_index);
            true
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Char('k' | 'K') => {
            move_create_picker(app, -1);
            true
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Char('j' | 'J') => {
            move_create_picker(app, 1);
            true
        }
        ParsedInput::Char(ch) if ch.is_ascii_alphabetic() => {
            let _ = open_create_modal_by_shortcut(app, *ch);
            true
        }
        _ => true,
    }
}

fn submit_create_modal(
    app: &mut App,
    game_kind: crate::app::rooms::svc::GameKind,
    display_name: String,
    settings: serde_json::Value,
) {
    if !can_create_room(app.is_admin) {
        app.banner = Some(Banner::error(
            "Admin only: creating rooms is locked for now.",
        ));
        app.rooms_create_flow = None;
        return;
    }

    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        app.banner = Some(Banner::error("Table name is required."));
        return;
    }
    let slug_prefix = app.room_game_registry.slug_prefix(game_kind);
    let label = app.room_game_registry.label(game_kind);

    app.rooms_service.create_game_room_task(
        app.user_id,
        game_kind,
        slug_prefix,
        label,
        display_name,
        settings,
    );
    app.rooms_create_flow = None;

    app.banner = Some(Banner::success(&format!(
        "Creating {} room.",
        app.room_game_registry.label(game_kind)
    )));
}

fn open_create_picker(app: &mut App) {
    if !can_create_room(app.is_admin) {
        app.banner = Some(Banner::error(
            "Admin only: creating rooms is locked for now.",
        ));
        return;
    }
    app.rooms_create_flow = Some(CreateRoomFlow::Picker { kind_index: 0 });
}

fn open_selected_create_modal(app: &mut App, kind_index: usize) {
    let Some(kind) = app
        .room_game_registry
        .ordered_kinds()
        .get(kind_index)
        .copied()
    else {
        return;
    };
    let modal = app.room_game_registry.open_create_modal(kind);
    app.rooms_create_flow = Some(CreateRoomFlow::Game { kind, modal });
}

fn open_create_modal_by_shortcut(app: &mut App, ch: char) -> bool {
    let target = ch.to_ascii_lowercase();
    let Some((index, _)) = app
        .room_game_registry
        .ordered_kinds()
        .iter()
        .enumerate()
        .find(|(_, kind)| {
            app.room_game_registry
                .label(**kind)
                .chars()
                .next()
                .is_some_and(|label_ch| label_ch.to_ascii_lowercase() == target)
        })
    else {
        return false;
    };
    open_selected_create_modal(app, index);
    true
}

fn move_create_picker(app: &mut App, delta: isize) {
    let len = app.room_game_registry.ordered_kinds().len();
    if let Some(CreateRoomFlow::Picker { kind_index }) = app.rooms_create_flow.as_mut() {
        *kind_index = cycle_index(*kind_index, len, delta);
    }
}

fn delete_selected_room(app: &mut App) {
    if !can_delete_room(app.is_admin) {
        app.banner = Some(Banner::error(
            "Admin only: deleting rooms is locked for now.",
        ));
        return;
    }

    let Some(room) = visible_real_room_at(app, app.rooms_selected_index) else {
        return;
    };

    app.rooms_service
        .delete_game_room_task(app.user_id, room.id, room.display_name.clone());
    app.banner = Some(Banner::success(&format!(
        "Deleting table: {}",
        room.display_name
    )));
}

fn handle_escape(app: &mut App) {
    if app.rooms_create_flow.is_some() {
        app.rooms_create_flow = None;
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

fn cycle_index(index: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
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
    let Some(room) = visible_real_room_at(app, app.rooms_selected_index) else {
        return;
    };
    let _ = enter_room(app, room);
}

pub(crate) fn enter_room(app: &mut App, room: crate::app::rooms::svc::RoomListItem) -> bool {
    if !can_enter_room(app.is_admin, app.is_moderator) {
        app.banner = Some(Banner::error("Rooms are locked for now."));
        return false;
    }

    app.chat.join_game_room_chat(room.chat_room_id);
    app.chat.request_room_tail(room.chat_room_id);
    app.rooms_service.touch_room_task(room.id);
    let same_room = app
        .active_room_game
        .as_ref()
        .is_some_and(|game| game.room_id() == room.id);
    if !same_room {
        app.active_room_game = Some(app.room_game_registry.enter(
            &room,
            app.user_id,
            app.chip_balance,
        ));
    }
    app.rooms_last_active_room_id = Some(room.id);
    app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Room);
    app.rooms_active_room = Some(room);
    app.rooms_create_flow = None;
    true
}

fn handle_active_room_key(app: &mut App, byte: u8) -> bool {
    let Some(room) = app.rooms_active_room.as_ref() else {
        return false;
    };
    let chat_room_id = room.chat_room_id;
    touch_active_room_activity(app);

    if byte == b'`' {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Room);
        app.set_screen(Screen::Dashboard);
        return true;
    }

    if byte == 0x1B
        && app
            .chat
            .selected_message_body_in_room(chat_room_id)
            .is_some()
    {
        app.chat.clear_message_selection();
        return true;
    }

    if should_route_active_room_chat_key(app, chat_room_id, byte)
        && crate::app::chat::input::handle_message_action_in_room(app, chat_room_id, byte)
    {
        return true;
    }

    let Some(active_room_game) = &mut app.active_room_game else {
        return false;
    };
    match active_room_game.handle_key(byte) {
        InputAction::Ignored => false,
        InputAction::Handled => true,
        InputAction::Leave => {
            app.rooms_active_room = None;
            true
        }
    }
}

fn handle_active_room_arrow(app: &mut App, key: u8) -> bool {
    let Some(room) = app.rooms_active_room.as_ref() else {
        return false;
    };
    let chat_room_id = room.chat_room_id;
    touch_active_room_activity(app);
    if crate::app::chat::input::handle_message_arrow_in_room(app, chat_room_id, key) {
        return true;
    }
    app.active_room_game
        .as_mut()
        .is_some_and(|game| game.handle_arrow(key))
}

fn handle_active_room_scroll(app: &mut App, delta: isize) -> bool {
    let Some(room) = app.rooms_active_room.as_ref() else {
        return false;
    };
    let chat_room_id = room.chat_room_id;
    touch_active_room_activity(app);
    crate::app::chat::input::handle_scroll_in_room(app, chat_room_id, delta);
    true
}

fn touch_active_room_activity(app: &mut App) {
    if let Some(active_room_game) = &app.active_room_game {
        active_room_game.touch_activity();
    }
}

fn active_room_page_step(app: &App) -> isize {
    (app.size.1 / 6).max(1) as isize
}

fn should_route_active_room_chat_key(app: &App, chat_room_id: uuid::Uuid, byte: u8) -> bool {
    if app.chat.is_reaction_leader_active() {
        return true;
    }
    if matches!(byte, b'i' | b'I' | b'j' | b'J' | b'k' | b'K' | 0x04 | 0x15) {
        return true;
    }
    let selected_in_room = app
        .chat
        .selected_message_body_in_room(chat_room_id)
        .is_some();
    selected_in_room
        && matches!(
            byte,
            b'd' | b'D'
                | b'r'
                | b'R'
                | b'e'
                | b'E'
                | b'p'
                | b'c'
                | b'f'
                | b'F'
                | b'g'
                | b'G'
                | b'\r'
                | b'\n'
                | 0x10
        )
}

fn can_create_room(is_admin: bool) -> bool {
    is_admin
}

fn can_delete_room(is_admin: bool) -> bool {
    is_admin
}

fn can_enter_room(is_admin: bool, is_moderator: bool) -> bool {
    let _ = (is_admin, is_moderator);
    true
}

#[cfg(test)]
mod tests {
    use super::{can_create_room, can_delete_room, can_enter_room};

    #[test]
    fn room_creation_stays_admin_only() {
        assert!(can_create_room(true));
        assert!(!can_create_room(false));
    }

    #[test]
    fn room_deletion_stays_admin_only() {
        assert!(can_delete_room(true));
        assert!(!can_delete_room(false));
    }

    #[test]
    fn room_entry_allows_all_users() {
        assert!(can_enter_room(true, false));
        assert!(can_enter_room(false, true));
        assert!(can_enter_room(true, true));
        assert!(can_enter_room(false, false));
    }
}
