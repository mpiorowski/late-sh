use crate::app::{common::primitives::Banner, state::App};

use super::data::ROOMS;

pub fn handle_key(app: &mut App, byte: u8) {
    if byte == 0x1B {
        if app.active_room.is_some() {
            app.active_room = None;
        }
        return;
    }

    match byte {
        b'j' | b'J' => {
            if app.active_room.is_some() {
                return;
            }
            app.room_selection = (app.room_selection + 1) % ROOMS.len();
        }
        b'k' | b'K' => {
            if app.active_room.is_some() {
                return;
            }
            app.room_selection = app.room_selection.saturating_add(ROOMS.len() - 1) % ROOMS.len();
        }
        b'\r' | b'\n' => {
            if let Some(room_idx) = app.active_room {
                let room = &ROOMS[room_idx.min(ROOMS.len() - 1)];
                app.banner = Some(Banner::success(&format!(
                    "Entered {}. Blackjack room wiring is next.",
                    room.slug
                )));
                return;
            }
            if !app.is_admin {
                app.banner = Some(Banner::error("Rooms are in progress for non-admin users"));
                return;
            }
            let room_idx = app.room_selection.min(ROOMS.len() - 1);
            let room_slug = ROOMS[room_idx].slug;
            app.active_room = Some(room_idx);
            app.banner = Some(Banner::success(&format!("Entered room {room_slug}")));
        }
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.active_room.is_some() {
        return false;
    }

    match key {
        b'A' => {
            app.room_selection = app.room_selection.saturating_add(ROOMS.len() - 1) % ROOMS.len();
            true
        }
        b'B' => {
            app.room_selection = (app.room_selection + 1) % ROOMS.len();
            true
        }
        _ => false,
    }
}
