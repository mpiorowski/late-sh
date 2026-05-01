use crate::app::{
    chat,
    common::{
        cli_install,
        primitives::{Banner, Screen},
    },
    rooms::svc::GameKind,
    state::{App, DashboardGameToggleTarget},
    vote,
};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    let Some(room_id) = app.dashboard_active_room_id() else {
        return false;
    };
    chat::input::handle_message_arrow_in_room(app, room_id, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.dashboard_blackjack_prefix_armed {
        app.dashboard_blackjack_prefix_armed = false;
        if let Some(slot) = blackjack_slot_for_key(byte) {
            return enter_blackjack_room_slot(app, slot);
        }
        // Any non-slot key disarms and continues through normal handling so
        // the second keystroke still does what the user typed.
    }

    // Dashboard favorite controls — all no-ops at <2 pins and fall
    // through as message-action input in that case.
    //   `[` / `]`   cycle prev / next through pinned favorites
    //   `,`         jump back to the previously-active pin
    //   `g<digit>`  two-key prefix to jump directly to slot 1..9
    let pins_len = app.profile_state.profile().favorite_room_ids.len();

    if app.dashboard_g_prefix_armed {
        app.dashboard_g_prefix_armed = false;
        if (b'1'..=b'9').contains(&byte) {
            app.jump_dashboard_favorite((byte - b'1') as usize);
            app.sync_visible_chat_room();
            return true;
        }
        // Any non-digit disarms and continues through normal handling so
        // the second keystroke isn't silently eaten.
    }

    if byte == b'g' && pins_len >= 2 {
        app.dashboard_g_prefix_armed = true;
        return true;
    }

    if byte == b'`' {
        return enter_last_game_room(app);
    }

    if byte == b'b'
        && app.profile_state.profile().show_dashboard_room_showcases
        && dashboard_blackjack_room_count(app) > 0
    {
        app.dashboard_blackjack_prefix_armed = true;
        return true;
    }

    if byte == b'[' {
        app.cycle_dashboard_favorite(-1);
        app.sync_visible_chat_room();
        return true;
    }
    if byte == b']' {
        app.cycle_dashboard_favorite(1);
        app.sync_visible_chat_room();
        return true;
    }
    if byte == b',' {
        app.toggle_dashboard_last_favorite();
        app.sync_visible_chat_room();
        return true;
    }
    if byte == b'B' {
        open_cli_install_modal(app);
        return true;
    }
    if byte == b'P' {
        open_browser_pairing_qr(app);
        return true;
    }

    let active_room_id = app.dashboard_active_room_id();

    if matches!(byte, b'i' | b'I')
        && let Some(room_id) = active_room_id
    {
        app.chat.start_composing_in_room(room_id);
        return true;
    }

    if byte == b'c'
        && let Some(room_id) = active_room_id
        && app.chat.selected_message_body_in_room(room_id).is_some()
    {
        return chat::input::handle_message_action_in_room(app, room_id, byte);
    }

    if vote::input::handle_key(app, byte) {
        return true;
    }

    if matches!(byte, b'\r' | b'\n')
        && let Some(room_id) = active_room_id
        && app.chat.try_jump_to_selected_reply_target_in_room(room_id)
    {
        return true;
    }

    let Some(room_id) = active_room_id else {
        return false;
    };
    chat::input::handle_message_action_in_room(app, room_id, byte)
}

pub(crate) fn open_cli_install_modal(app: &mut App) {
    app.pending_clipboard = Some(cli_install::INSTALL_COMMAND.to_string());
    app.show_web_chat_qr = false;
    app.web_chat_qr_url = None;
    app.show_cli_install_modal = true;
}

pub(crate) fn open_browser_pairing_qr(app: &mut App) {
    app.pending_clipboard = Some(app.connect_url.clone());
    app.web_chat_qr_url = Some(app.connect_url.clone());
    app.show_cli_install_modal = false;
    app.show_web_chat_qr = true;
}

fn dashboard_blackjack_room_count(app: &App) -> usize {
    app.rooms_snapshot
        .rooms
        .iter()
        .filter(|room| matches!(room.game_kind, GameKind::Blackjack))
        .count()
}

fn enter_blackjack_room_slot(app: &mut App, slot: usize) -> bool {
    let Some(room) = app
        .rooms_snapshot
        .rooms
        .iter()
        .filter(|room| matches!(room.game_kind, GameKind::Blackjack))
        .nth(slot)
        .cloned()
    else {
        return false;
    };

    if crate::app::rooms::input::enter_room(app, room) {
        app.set_screen(Screen::Rooms);
        true
    } else {
        false
    }
}

fn enter_last_game_room(app: &mut App) -> bool {
    if app.dashboard_game_toggle_target == Some(DashboardGameToggleTarget::Arcade)
        && app.is_playing_game
    {
        app.set_screen(Screen::Games);
        return true;
    }

    let room = app.rooms_active_room.clone().or_else(|| {
        let room_id = app.rooms_last_active_room_id?;
        app.rooms_snapshot
            .rooms
            .iter()
            .find(|room| room.id == room_id)
            .cloned()
    });
    let Some(room) = room else {
        if app.is_playing_game {
            app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
            app.set_screen(Screen::Games);
        } else {
            app.banner = Some(Banner::error("No game to return to."));
        }
        return true;
    };

    if crate::app::rooms::input::enter_room(app, room) {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Room);
        app.set_screen(Screen::Rooms);
    }
    true
}

pub(crate) fn blackjack_slot_for_key(byte: u8) -> Option<usize> {
    match byte {
        b'1'..=b'3' => Some((byte - b'1') as usize),
        _ => None,
    }
}
