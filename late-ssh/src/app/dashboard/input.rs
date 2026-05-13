use crate::app::{
    chat,
    common::primitives::{Banner, Screen},
    state::{App, DashboardGameToggleTarget},
    vote,
};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    chat::input::handle_arrow(app, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.vote_prefix_armed {
        app.vote_prefix_armed = false;
        if vote::input::handle_vote_suffix(app, byte) {
            return true;
        }
    }

    if byte == b'`' {
        return enter_last_game_room(app);
    }

    if byte == b'P' {
        open_pair_modal(app);
        return true;
    }

    if vote::input::handle_key(app, byte) {
        return true;
    }

    chat::input::handle_byte(app, byte)
}

pub(crate) fn open_pair_modal(app: &mut App) {
    app.show_web_chat_qr = false;
    app.web_chat_qr_url = None;
    app.show_pair_modal = true;
}

fn enter_last_game_room(app: &mut App) -> bool {
    if app.dashboard_game_toggle_target == Some(DashboardGameToggleTarget::Arcade)
        && app.is_playing_game
    {
        app.set_screen(Screen::Arcade);
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
            app.set_screen(Screen::Arcade);
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
