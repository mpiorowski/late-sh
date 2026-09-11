//! Keys for the Zen page: the chat keys of the focused chat tile, the tank
//! feed, and the layout keys. The bonsai is tended in its care modal, the
//! same one `w` opens on every other page, so no care key is captured
//! here. Anything not owned here returns `false` so the global keys
//! (digits, Tab, `q`, `?`, `w`, the `v` music chords) keep working.

use uuid::Uuid;

use super::state::{Dir, MAX_TILES, TileKind};
use crate::app::{common::primitives::Banner, input::ParsedInput, state::App};

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // A pressed `v` owns the next key everywhere; the page must not eat the
    // suffix (`v x` swaps the audio source, `X` here closes a tile).
    if app.music_prefix_armed {
        return false;
    }
    if handle_common(app, event) {
        return true;
    }
    handle_rice(app, event)
}

/// The chat keys and the tank feed `a`. The chat keys belong to the
/// focused chat tile: `[` `]` rebind it to the previous or next joined
/// room, `i` and Enter write in its room, `j` `k` select in it, and the
/// message actions act on its selection; with any other tile focused all
/// of them are swallowed, so a page of several chats never scrolls one you
/// are not looking at. The sprout is cut on its Shop row, on purpose: no
/// page key for it. The pet has no key at all: it is petted
/// with a click and reads the session for the rest.
fn handle_common(app: &mut App, event: &ParsedInput) -> bool {
    let Some(byte) = event_byte(event) else {
        return false;
    };
    let chat_focused = app.zen.focused_kind() == Some(TileKind::Chat);
    // The focused chat tile's message keys, the way the house table routes
    // them to its embedded chat: `i`, `j` `k`, Ctrl+D/U, and the reaction
    // leader always; `d` `r` `e` `p` `c` `t` `G` and Enter only while a
    // message in that room is selected, so `r` flips the tile otherwise.
    if chat_focused && let Some(room_id) = app.zen_chat_room_id() {
        if crate::app::chat::input::chat_priority_key(app, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, room_id, byte)
        {
            return true;
        }
        if crate::app::chat::input::selected_chat_key(app, room_id, byte)
            && crate::app::chat::input::handle_message_action_in_room(app, room_id, byte)
        {
            return true;
        }
    }
    match byte {
        b'[' => {
            if chat_focused {
                cycle_room(app, -1);
            }
            true
        }
        b']' => {
            if chat_focused {
                cycle_room(app, 1);
            }
            true
        }
        b'i' | b'\r' | b'\n' => {
            if chat_focused && let Some(room_id) = app.zen_chat_room_id() {
                app.chat.start_composing_in_room(room_id);
            }
            true
        }
        // Swallowed unless a chat is focused; then the global handler selects.
        b'j' | b'J' | b'k' | b'K' => !chat_focused,
        b'a' => {
            crate::app::input::feed_aquarium_globally(app);
            true
        }
        _ => false,
    }
}

/// Rice: arrows move focus and the layout keys edit the tree. Every edit
/// marks the layout for the debounced write (`App::flush_zen_layout`).
fn handle_rice(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Arrow(b'D') | ParsedInput::Arrow(b'A') => {
            app.zen.focus_prev();
            focus_moved(app);
            return true;
        }
        ParsedInput::Arrow(b'C') | ParsedInput::Arrow(b'B') => {
            app.zen.focus_next();
            focus_moved(app);
            return true;
        }
        _ => {}
    }
    let Some(byte) = event_byte(event) else {
        return false;
    };
    let changed = match byte {
        b' ' => app.zen.cycle_focused_kind(true),
        b'S' => {
            if app.zen.leaf_count() >= MAX_TILES {
                app.banner = Some(Banner::info("That is every tile the page holds"));
                return true;
            }
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
        app.sync_visible_chat_room();
        app.mark_zen_layout_dirty();
    }
    true
}

/// The focus landed somewhere else: the chat that reads as visible (marked
/// read, tail kept fresh) is the focused chat tile's, and a selection or a
/// draft belongs to the tile it was made in. A draft written for another
/// room is closed: every submit goes to the room the composer was opened
/// in, and the active tile now draws the composer under its own label, so
/// the two must never differ.
pub(crate) fn focus_moved(app: &mut App) {
    app.chat.clear_message_selection();
    if app.chat.composing && app.chat.composer_room_id() != app.zen_chat_room_id() {
        app.chat.reset_composer();
    }
    app.sync_visible_chat_room();
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

/// Walk the joined rooms in rail order and bind the focused chat tile to
/// the one landed on; the binding is part of the layout, so it is saved.
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
    if app.zen.bind_focused_chat_room(Some(ids[next])) {
        app.chat.reset_composer();
        app.chat.clear_message_selection();
        app.sync_visible_chat_room();
        app.mark_zen_layout_dirty();
    }
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
