use crate::app::{
    chat,
    common::primitives::{Banner, Screen},
    rooms::svc::RoomListItem,
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
        return cycle_game_workspace(app);
    }

    if vote::input::handle_key(app, byte) {
        return true;
    }

    chat::input::handle_byte(app, byte)
}

pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    match app.screen {
        Screen::Dashboard => enter_first_game_workspace(app),
        Screen::Rooms if app.rooms_active_room.is_some() => enter_next_room_workspace(app),
        Screen::Arcade if app.is_playing_game => {
            app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
            app.set_screen(Screen::Dashboard);
            true
        }
        _ => false,
    }
}

fn enter_first_game_workspace(app: &mut App) -> bool {
    let room = seated_room_workspaces(app).into_iter().next();
    if let Some(room) = room {
        if crate::app::rooms::input::enter_room(app, room) {
            app.set_screen(Screen::Rooms);
        }
    } else if app.is_playing_game {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
        app.set_screen(Screen::Arcade);
    } else {
        app.banner = Some(Banner::error("No seated tables."));
    }
    true
}

fn enter_next_room_workspace(app: &mut App) -> bool {
    let Some(active_room_id) = app.rooms_active_room.as_ref().map(|room| room.id) else {
        app.set_screen(Screen::Dashboard);
        return true;
    };
    let rooms = seated_room_workspaces(app);
    let next_room = rooms
        .iter()
        .position(|room| room.id == active_room_id)
        .and_then(|index| rooms.get(index + 1))
        .cloned();

    if let Some(room) = next_room {
        if crate::app::rooms::input::enter_room(app, room) {
            app.set_screen(Screen::Rooms);
        }
    } else {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Room);
        app.set_screen(Screen::Dashboard);
    }
    true
}

fn seated_room_workspaces(app: &App) -> Vec<RoomListItem> {
    app.rooms_snapshot
        .rooms
        .iter()
        .filter(|room| app.room_game_registry.is_user_seated(room, app.user_id))
        .cloned()
        .collect()
}
