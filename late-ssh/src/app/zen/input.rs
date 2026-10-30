//! Keys for the Zen page: the chat keys of the focused chat tile, the tank
//! feed, and the layout keys. The bonsai is tended in its care modal, the
//! same one `w` opens on every other page, so no care key is captured
//! here. Tab and Shift+Tab are the page's: they walk the tile focus, since
//! Zen is not in the page cycle and the global Tab would drop back to Home.
//! Anything not owned here returns `false` so the global keys (digits,
//! `q`, `?`, `w`, the `v` music chords) keep working.

use std::time::Instant;

use uuid::Uuid;

use super::rows::InboxRow;
use super::state::{Dir, GuestPick, KindPick, MAX_TILES, TileKind};
use crate::app::{
    chat::state::RoomSlot, common::primitives::Banner, input::ParsedInput, state::App,
};

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // A pressed `v` owns the next key everywhere; the page must not eat the
    // suffix (`v x` swaps the audio source, `X` here closes a tile).
    if app.music_prefix_armed {
        return false;
    }
    // The pickers own every key while one is up (Esc closes it in
    // `dispatch_escape`), so nothing under it moves.
    if app.rice().kind_picker.is_some() {
        handle_kind_picker(app, event);
        return true;
    }
    if app.rice().guest_picker.is_some() {
        handle_guest_picker(app, event);
        return true;
    }
    if handle_common(app, event) {
        return true;
    }
    handle_rice(app, event)
}

/// Home's keys for its tiles, ahead of Home's own handling. In arrange
/// mode (`Ctrl+E`) the layout keys are live and every tile takes the
/// focus; outside it only a focused chat tile has keys of its own (the
/// chat keys of its room, as on Zen), and everything else falls through
/// to Home, whose center is the current room.
pub fn handle_home_event(app: &mut App, event: &ParsedInput) -> bool {
    if app.music_prefix_armed {
        return false;
    }
    if app.rice().kind_picker.is_some() {
        handle_kind_picker(app, event);
        return true;
    }
    if app.rice().guest_picker.is_some() {
        handle_guest_picker(app, event);
        return true;
    }
    if app.rice().arrange {
        // Arrange mode is a layout mode: a focused chat tile's room keys
        // (`[` `]`, Ctrl+/) still bind it, the rest of the chat waits.
        if let Some(byte) = event_byte(event)
            && matches!(byte, b'[' | b']')
        {
            cycle_tile(app, app.rice().chat_tile_focused(), byte == b']');
            return true;
        }
        if handle_rice(app, event) {
            return true;
        }
        // Every other key is swallowed, so a stray letter never reaches
        // the chat while the page is being arranged.
        return event_byte(event).is_some();
    }
    // Outside arrange mode the active chat tile is Home's chat, and the
    // rail its room, so Home's own keys serve it.
    false
}

/// Enter or leave Home's arrange mode. Leaving writes the layout at once
/// and closes the pickers; entering says what the keys are now.
pub(crate) fn toggle_home_arrange(app: &mut App) {
    let arrange = !app.rice().arrange;
    let rice = app.rice_mut();
    rice.arrange = arrange;
    rice.close_kind_picker();
    rice.close_guest_picker();
    // `z` is a preview while arranging; the page leaves the mode whole.
    if !arrange {
        rice.zoomed = false;
    }
    if arrange {
        app.banner = Some(Banner::info(
            "Arrange: arrows move · S split · X close · F flip · <>{} resize · space tile · + guests · R reset · Esc done",
        ));
    } else {
        // Leaving puts the focus back on a chat, the only tiles that keep
        // it outside arrange mode.
        if !app.rice().focused_kind().is_some_and(TileKind::takes_focus)
            && let Some(ordinal) = app.rice().first_tile_of(TileKind::Chat)
        {
            app.rice_mut().focus = ordinal;
            focus_moved(app);
        }
        app.flush_rice_layouts();
    }
}

/// The chat keys, the Live tile's `s` prefix, the ascii tile's pieces, and
/// the tank feed `a`. `[` `]` on a focused ascii tile step it through the
/// pieces and Enter opens the picker over them. The chat keys belong to the
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
    // The Live prefix, as on Home: `s` then a row's number opens that row
    // of the Live tile, from whichever tile has the focus, while the page
    // draws one (one zoomed away is not shown). Any other key after `s` is
    // swallowed, as on Home, so it is spent here, ahead of the focused
    // tile's own keys.
    if app.live_prefix_armed {
        app.live_prefix_armed = false;
        crate::app::live::input::open_from_prefix(app, byte);
        return true;
    }
    if app.rice().focused_kind() == Some(TileKind::Inbox) && handle_inbox(app, byte) {
        return true;
    }
    if app.rice().focused_kind() == Some(TileKind::Headlines) && handle_headlines(app, byte) {
        return true;
    }
    let chat_focused = app.rice().focused_kind() == Some(TileKind::Chat);
    // The focused chat tile's message keys, the way the house table routes
    // them to its embedded chat: `i`, `j` `k`, Ctrl+D/U, and the reaction
    // leader always; `d` `r` `e` `p` `c` `t` `G` and Enter only while a
    // message in that room is selected. The layout keys are uppercase
    // (`S` `X` `F` `Z` `R`), so none of these collide with them.
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
    // Lowercase only: `S` is the split.
    if byte == b's' && app.rice().draws(TileKind::Live) {
        app.live_prefix_armed = true;
        return true;
    }
    match byte {
        b'[' => {
            cycle_tile(app, chat_focused, false);
            true
        }
        b']' => {
            cycle_tile(app, chat_focused, true);
            true
        }
        b'i' | b'\r' | b'\n' => {
            if chat_focused && let Some(room_id) = app.zen_chat_room_id() {
                app.chat.start_composing_in_room(room_id);
            }
            // Enter on an ascii tile picks its piece from the list.
            if byte != b'i' && app.rice().focused_kind() == Some(TileKind::Ascii) {
                crate::app::ascii::picker::input::open(app);
            }
            true
        }
        // Swallowed unless a chat is focused; then the global handler selects.
        b'j' | b'J' | b'k' | b'K' => !chat_focused,
        b'a' => {
            crate::app::input::feed_aquarium_globally(app);
            true
        }
        // The backtick chain, as on Home: it hops through the games waiting
        // on you and comes home here.
        b'`' => crate::app::workspace::cycle::cycle_game_workspace(app),
        _ => false,
    }
}

/// `[` `]` on the focused tile: a chat walks its joined rooms, an ascii tile
/// its pieces. Any other tile takes the key and does nothing.
fn cycle_tile(app: &mut App, chat_focused: bool, forward: bool) {
    match chat_focused {
        true => cycle_room(app, if forward { 1 } else { -1 }),
        false => {
            if app
                .zen
                .cycle_focused_piece(forward, Instant::now(), &mut rand::thread_rng())
            {
                app.mark_zen_layout_dirty();
            }
        }
    }
}

/// The focused Inbox tile: `j` `k` walk its rows, Enter opens the row.
fn handle_inbox(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            let last = inbox_rows(app).len().saturating_sub(1);
            app.rice_mut().inbox_selected = (app.rice().inbox_selected + 1).min(last);
            true
        }
        b'k' | b'K' => {
            app.rice_mut().inbox_selected = app.rice().inbox_selected.saturating_sub(1);
            true
        }
        b'\r' | b'\n' => {
            open_inbox_row(app);
            true
        }
        _ => false,
    }
}

fn inbox_rows(app: &App) -> Vec<InboxRow> {
    super::rows::inbox_rows(
        app.user_id,
        &app.chat.rooms,
        &app.chat.unread_counts,
        app.chat.usernames(),
        app.chat.ignored_user_ids(),
        app.chat.notifications.all_items(),
    )
}

/// Open the selected Inbox row in the page's first chat tile and move the
/// focus there, so `i` answers at once: a DM binds the tile to its room, a
/// mention binds it to the mention's room and selects the message the way
/// a `Ctrl+/` message jump does. A mention in a room the account never
/// joined has no tile to land in and reads in the history modal instead.
fn open_inbox_row(app: &mut App) {
    let rows = inbox_rows(app);
    let Some(row) = rows.get(app.rice().inbox_selected.min(rows.len().saturating_sub(1))) else {
        return;
    };
    let (room_id, message_id) = match row {
        InboxRow::Dm { room_id, .. } => (*room_id, None),
        InboxRow::Mention {
            room_id,
            message_id,
            ..
        } => (*room_id, Some(*message_id)),
    };
    if let Some(message_id) = message_id
        && !app.chat.rooms.iter().any(|(room, _)| room.id == room_id)
    {
        app.chat.open_history_at_message(room_id, message_id);
        return;
    }
    let Some(chat_tile) = app.rice().first_tile_of(TileKind::Chat) else {
        app.banner = Some(Banner::info("Add a chat tile to open it here"));
        return;
    };
    app.rice_mut().focus = chat_tile;
    focus_moved(app);
    bind_active_chat_to_room(app, room_id);
    let Some(message_id) = message_id else {
        return;
    };
    if app.chat.message_is_loaded_in_room(room_id, message_id) {
        app.chat.select_message_by_id_in_room(room_id, message_id);
    } else {
        app.chat.set_pending_search_jump(room_id, message_id);
        app.chat.request_room_tail(room_id);
    }
}

/// The focused Headlines tile: `j` `k` walk its items, Enter copies the
/// selected link. The rows are the ones the tile draws, so the marked item
/// is the one copied.
fn handle_headlines(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            let last = headline_rows(app).len().saturating_sub(1);
            app.rice_mut().headlines_selected = (app.rice().headlines_selected + 1).min(last);
            true
        }
        b'k' | b'K' => {
            app.rice_mut().headlines_selected = app.rice().headlines_selected.saturating_sub(1);
            true
        }
        b'\r' | b'\n' => {
            let rows = headline_rows(app);
            if let Some(row) = rows.get(
                app.rice()
                    .headlines_selected
                    .min(rows.len().saturating_sub(1)),
            ) {
                app.pending_clipboard = Some(row.url.clone());
                app.banner = Some(Banner::success("Link copied to clipboard!"));
            }
            true
        }
        _ => false,
    }
}

fn headline_rows(app: &App) -> Vec<super::rows::Headline> {
    super::rows::headlines(app.chat.news.all_articles(), app.chat.feeds.all_entries())
}

/// Rice: arrows, Tab, and Shift+Tab move focus and the layout keys edit
/// the tree. Every edit marks the layout for the debounced write
/// (`App::flush_zen_layout`).
fn handle_rice(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Arrow(b'D') | ParsedInput::Arrow(b'A') | ParsedInput::BackTab => {
            app.rice_mut().focus_prev();
            focus_moved(app);
            return true;
        }
        ParsedInput::Arrow(b'C') | ParsedInput::Arrow(b'B') => {
            app.rice_mut().focus_next();
            focus_moved(app);
            return true;
        }
        _ => {}
    }
    let Some(byte) = event_byte(event) else {
        return false;
    };
    let changed = match byte {
        b'\t' => {
            app.rice_mut().focus_next();
            focus_moved(app);
            return true;
        }
        b' ' => {
            app.rice_mut().open_kind_picker();
            return true;
        }
        b'+' => {
            app.rice_mut().open_guest_picker();
            return true;
        }
        b'S' => {
            if app.rice().leaf_count() >= MAX_TILES {
                app.banner = Some(Banner::info("That is every tile the page holds"));
                return true;
            }
            let wide = focused_tile_is_wide(app);
            app.rice_mut().split_focused(wide)
        }
        b'X' => {
            if app.rice().leaf_count() <= 1 {
                app.banner = Some(Banner::info("The last tile stays"));
                return true;
            }
            app.rice_mut().close_focused()
        }
        b'<' | b',' => resize_or_explain(app, Dir::Row, -1),
        b'>' | b'.' => resize_or_explain(app, Dir::Row, 1),
        b'{' => resize_or_explain(app, Dir::Column, -1),
        b'}' => resize_or_explain(app, Dir::Column, 1),
        b'F' => app.rice_mut().flip_focused(),
        b'z' => {
            app.rice_mut().toggle_zoom();
            // The zoom is a view, not a layout edit; still resizes the tank.
            app.sync_aquarium_bounds();
            return true;
        }
        b'b' => {
            app.rice_mut().cycle_border();
            true
        }
        b'g' => {
            app.rice_mut().cycle_gap();
            true
        }
        b't' => {
            app.rice_mut().toggle_titles();
            true
        }
        b'R' => {
            app.rice_mut().reset();
            true
        }
        _ => return false,
    };
    if changed {
        app.sync_aquarium_bounds();
        // An edit can land the focus, and so the active chat, elsewhere.
        focus_moved(app);
        app.mark_zen_layout_dirty();
    }
    true
}

/// The tile picker: `j` `k` and the arrows move, Enter or `space` picks,
/// everything else is swallowed. A refused row (Chat past the cap) says
/// so in a banner and leaves the picker up.
fn handle_kind_picker(app: &mut App, event: &ParsedInput) {
    let byte = event_byte(event);
    match (event, byte) {
        (ParsedInput::Arrow(b'A'), _) | (_, Some(b'k')) => app.rice_mut().move_kind_picker(-1),
        (ParsedInput::Arrow(b'B'), _) | (_, Some(b'j')) => app.rice_mut().move_kind_picker(1),
        (_, Some(b'\r' | b'\n' | b' ')) => match app.rice_mut().pick_kind() {
            KindPick::Changed => {
                app.pin_chat_tiles_on_pick();
                app.sync_aquarium_bounds();
                focus_moved(app);
                app.mark_zen_layout_dirty();
            }
            KindPick::Unchanged => {}
            KindPick::ChatFull => {
                app.banner = Some(Banner::info("That is every chat tile the page holds"));
            }
        },
        _ => {}
    }
}

/// The guest picker: `j` `k` and the arrows move, Enter or `space`
/// toggles the row's guest on the focused tile and keeps the picker up,
/// everything else is swallowed.
fn handle_guest_picker(app: &mut App, event: &ParsedInput) {
    let byte = event_byte(event);
    match (event, byte) {
        (ParsedInput::Arrow(b'A'), _) | (_, Some(b'k')) => app.rice_mut().move_guest_picker(-1),
        (ParsedInput::Arrow(b'B'), _) | (_, Some(b'j')) => app.rice_mut().move_guest_picker(1),
        (_, Some(b'\r' | b'\n' | b' ')) => match app.rice_mut().pick_guest() {
            GuestPick::Changed => {
                app.sync_aquarium_bounds();
                app.mark_zen_layout_dirty();
            }
            GuestPick::Unchanged => {}
        },
        _ => {}
    }
}

/// The focus landed somewhere else: the chat that reads as visible (marked
/// read, tail kept fresh) is the focused chat tile's, and a selection or a
/// draft belongs to the tile it was made in. A draft written for another
/// room is closed: every submit goes to the room the composer was opened
/// in, and the active tile now draws the composer under its own label, so
/// the two must never differ.
pub(crate) fn focus_moved(app: &mut App) {
    app.chat.clear_message_selection();
    app.sync_rail_to_active_chat();
    if app.chat.composing && app.chat.composer_room_id() != app.zen_chat_room_id() {
        app.chat.reset_composer();
    }
    app.sync_visible_chat_room();
}

/// Move the focused tile's edge by `delta_cells` along `dir`, or say why
/// nothing moved: a tile with no split of that direction above it has
/// nothing to trade.
fn resize_or_explain(app: &mut App, dir: Dir, delta_cells: i16) -> bool {
    let tiles_area = app.rice_tiles_area();
    if app.rice_mut().resize_focused(dir, delta_cells, tiles_area) {
        return true;
    }
    let axis = match dir {
        Dir::Row => "width",
        Dir::Column => "height",
    };
    app.banner = Some(Banner::info(&format!(
        "No split to trade {axis} with; S splits, F flips"
    )));
    false
}

/// Whether the focused tile is wider than tall in cells (a cell is about
/// twice as tall as wide, so width is halved before comparing).
fn focused_tile_is_wide(app: &App) -> bool {
    let tiles_area = app.rice_tiles_area();
    let rects =
        super::layout::tile_rects(&app.rice().rice.root, tiles_area, app.rice().gaps(), None);
    rects
        .get(app.rice().focus)
        .map(|(_, rect)| rect.width / 2 >= rect.height)
        .unwrap_or(true)
}

/// Walk the rooms in rail order, top to bottom, and bind the focused chat
/// tile to the one landed on; the binding is part of the layout, so it is
/// saved. The rail's synthetic rows (mentions, rss, voice, browse) are not
/// rooms a tile can show and are stepped over, and so are the rooms of a
/// collapsed section, which the rail does not show either.
fn cycle_room(app: &mut App, delta: isize) {
    let ids: Vec<Uuid> = app
        .chat
        .visual_order()
        .into_iter()
        .filter_map(|slot| match slot {
            RoomSlot::Room(room_id) => Some(room_id),
            RoomSlot::Feeds
            | RoomSlot::News
            | RoomSlot::Cyberspace
            | RoomSlot::CyberspaceNotifications
            | RoomSlot::CyberspaceMail(_)
            | RoomSlot::CyberspaceRoom(_)
            | RoomSlot::Notifications
            | RoomSlot::Discover
            | RoomSlot::Showcase
            | RoomSlot::Work => None,
        })
        .collect();
    if ids.is_empty() {
        return;
    }
    let current = app
        .zen_chat_room_id()
        .and_then(|id| ids.iter().position(|room_id| *room_id == id))
        .unwrap_or(0);
    let next = (current as isize + delta).rem_euclid(ids.len() as isize) as usize;
    bind_active_chat_to_room(app, ids[next]);
}

/// Bind the active chat tile to `room_id` (a layout edit, saved), move
/// the rail there, and drop the draft and selection that belonged to its
/// old room. `false` when the page has no chat tile, so the caller can
/// fall back to Home's selection; the `Ctrl+/` picker and `[` `]` both
/// land here.
pub(crate) fn bind_active_chat_to_room(app: &mut App, room_id: Uuid) -> bool {
    if !app.rice_mut().bind_active_chat_room(Some(room_id)) {
        return false;
    }
    app.chat.reset_composer();
    app.chat.clear_message_selection();
    app.sync_rail_to_active_chat();
    app.sync_visible_chat_room();
    app.mark_zen_layout_dirty();
    true
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
