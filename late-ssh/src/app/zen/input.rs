//! Keys for both Zen pages. Care keys are the same on both (they act on the
//! things you own, wherever they sit); Rice adds the layout keys. Anything
//! not owned here returns `false` so the global keys (digits, Tab, `q`,
//! `?`, the `v` music chords) keep working.

use uuid::Uuid;

use super::state::{Dir, TileKind};
use crate::app::{
    chat::state::RoomSlot, common::primitives::Banner, input::ParsedInput, state::App,
};

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    if handle_common(app, event) {
        return true;
    }
    handle_rice(app, event)
}

/// Compose, the room walk, and the care keys: bonsai
/// `w x p s n N`, pet `f d`, tank `a`.
fn handle_common(app: &mut App, event: &ParsedInput) -> bool {
    let Some(byte) = event_byte(event) else {
        return false;
    };
    match byte {
        b'[' => {
            cycle_room(app, -1);
            true
        }
        b']' => {
            cycle_room(app, 1);
            true
        }
        b'i' | b'\r' | b'\n' => {
            if let Some(room_id) = app.zen_chat_room_id() {
                app.chat.start_composing_in_room(room_id);
            }
            true
        }
        b'w' => {
            water_bonsai(app);
            true
        }
        b'x' => {
            app.bonsai_state.prune_selected();
            true
        }
        b'p' => {
            app.bonsai_state.pinch_selected();
            true
        }
        b's' => {
            app.bonsai_state.split_selected();
            true
        }
        b'n' => {
            app.bonsai_state.cycle_selection(1);
            true
        }
        b'N' => {
            app.bonsai_state.cycle_selection(-1);
            true
        }
        b'f' => {
            crate::app::input::pet_feed_globally(app);
            true
        }
        b'a' => {
            crate::app::input::feed_aquarium_globally(app);
            true
        }
        _ => false,
    }
}

/// Rice: arrows move focus, hjkl steer the tree when a bonsai tile has
/// focus, and the layout keys edit the tree. Every edit persists.
fn handle_rice(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Arrow(b'D') | ParsedInput::Arrow(b'A') => {
            app.zen.focus_prev();
            return true;
        }
        ParsedInput::Arrow(b'C') | ParsedInput::Arrow(b'B') => {
            app.zen.focus_next();
            return true;
        }
        _ => {}
    }
    let Some(byte) = event_byte(event) else {
        return false;
    };
    let bonsai_focused = app.zen.focused_kind() == Some(TileKind::Bonsai);
    let changed = match byte {
        b'h' if bonsai_focused => {
            app.bonsai_state.bend_selected(-1, 0);
            return true;
        }
        b'l' if bonsai_focused => {
            app.bonsai_state.bend_selected(1, 0);
            return true;
        }
        b'k' if bonsai_focused => {
            app.bonsai_state.bend_selected(0, 1);
            return true;
        }
        b'j' if bonsai_focused => {
            app.bonsai_state.bend_selected(0, -1);
            return true;
        }
        b' ' => app.zen.cycle_focused_kind(true),
        b'S' => {
            let wide = focused_tile_is_wide(app);
            app.zen.split_focused(wide)
        }
        b'X' => {
            if app.zen.leaf_count() <= 1 {
                app.banner = Some(Banner::info("The last tile stays"));
                return true;
            }
            app.zen.close_focused()
        }
        b'<' | b',' => resize_or_explain(app, Dir::Row, -1),
        b'>' | b'.' => resize_or_explain(app, Dir::Row, 1),
        b'{' => resize_or_explain(app, Dir::Column, -1),
        b'}' => resize_or_explain(app, Dir::Column, 1),
        b'r' => app.zen.flip_focused(),
        b'z' => {
            app.zen.toggle_zoom();
            // The zoom is a view, not a layout edit; still resizes the tank.
            app.sync_aquarium_bounds();
            return true;
        }
        b'b' => {
            app.zen.cycle_border();
            true
        }
        b'g' => {
            app.zen.cycle_gap();
            true
        }
        b't' => {
            app.zen.toggle_titles();
            true
        }
        b'R' => {
            app.zen.reset();
            true
        }
        _ => return false,
    };
    if changed {
        app.sync_aquarium_bounds();
        app.persist_zen_layout();
    }
    true
}

/// Move the focused tile's edge by `delta_cells` along `dir`, or say why
/// nothing moved: a tile with no split of that direction above it has
/// nothing to trade.
fn resize_or_explain(app: &mut App, dir: Dir, delta_cells: i16) -> bool {
    let (cols, rows) = app.size;
    let (tiles_area, _) = super::layout::rice_areas(ratatui::layout::Rect::new(0, 0, cols, rows));
    if app.zen.resize_focused(dir, delta_cells, tiles_area) {
        return true;
    }
    let axis = match dir {
        Dir::Row => "width",
        Dir::Column => "height",
    };
    app.banner = Some(Banner::info(&format!(
        "No split to trade {axis} with; S splits, r flips"
    )));
    false
}

/// Whether the focused tile is wider than tall in cells (a cell is about
/// twice as tall as wide, so width is halved before comparing).
fn focused_tile_is_wide(app: &App) -> bool {
    let (cols, rows) = app.size;
    let (tiles_area, _) = super::layout::rice_areas(ratatui::layout::Rect::new(0, 0, cols, rows));
    let rects = super::layout::tile_rects(
        &app.zen.rice.root,
        tiles_area,
        app.zen.rice.look.gap as u16,
        None,
    );
    rects
        .get(app.zen.focus)
        .map(|(_, rect)| rect.width / 2 >= rect.height)
        .unwrap_or(true)
}

/// Walk the joined rooms in rail order; the pick lands in Home's selection,
/// which is what this page shows.
fn cycle_room(app: &mut App, delta: isize) {
    let ids: Vec<Uuid> = app.chat.rooms.iter().map(|(room, _)| room.id).collect();
    if ids.is_empty() {
        return;
    }
    let current = app
        .zen_chat_room_id()
        .and_then(|id| ids.iter().position(|room_id| *room_id == id))
        .unwrap_or(0);
    let next = (current as isize + delta).rem_euclid(ids.len() as isize) as usize;
    app.chat.select_room_slot(RoomSlot::Room(ids[next]));
    app.sync_visible_chat_room();
}

fn water_bonsai(app: &mut App) {
    // The first `w` on a dead tree replants; watering starts on the next.
    if !app.bonsai_state.is_alive {
        app.bonsai_state.respawn();
        return;
    }
    app.bonsai_state.water();
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
