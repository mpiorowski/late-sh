//! Keys for Home's tiles: arrange mode's layout keys, the keys of the
//! focused tile while arranging (the attention guest, the Inbox and
//! Headlines rows, an ascii tile's pieces, the tank's feed), and Home's
//! named layouts (the digit, the `L` list, `/layout`, `/zen`). Outside
//! arrange mode Home's own keys serve the page's chat, so nothing here
//! runs. In arrange mode a key not owned here is swallowed, so a stray
//! letter never reaches the chat; the `v` music chords still pass.

use std::time::Instant;

use uuid::Uuid;

use super::presets::{LayoutCommand, LayoutList, Layouts, LayoutsError, NameEdit, NamePurpose};
use super::rows::InboxRow;
use super::state::{Dir, Guest, GuestPick, KindPick, MAX_TILES, TileKind};
use crate::app::attention::state::Item as AttentionItem;
use crate::app::{
    chat::state::RoomSlot, common::primitives::Banner, input::ParsedInput, state::App,
};

/// Home's keys for its tiles, ahead of Home's own handling. In arrange
/// mode (`Ctrl+F`, `Ctrl+E`) the layout keys are live and every tile takes
/// the focus; outside it the active chat tile is Home's chat and
/// everything falls through to Home, whose center is that tile's room.
pub fn handle_home_event(app: &mut App, event: &ParsedInput) -> bool {
    // A pressed `v` owns the next key everywhere; the page must not eat the
    // suffix (`v x` swaps the audio source, `X` here closes a tile).
    if app.music_prefix_armed {
        return false;
    }
    // The pickers own every key while one is up (Esc closes it in
    // `dispatch_escape`), so nothing under it moves.
    if app.home.kind_picker.is_some() {
        handle_kind_picker(app, event);
        return true;
    }
    if app.home.guest_picker.is_some() {
        handle_guest_picker(app, event);
        return true;
    }
    if app.home.arrange {
        // The layouts list owns every key while it is up (Esc closes it
        // in `dispatch_escape`).
        if app.layout_list.is_some() {
            handle_layout_list(app, event);
            return true;
        }
        // Arrange mode is a layout mode: a focused chat tile's room keys
        // (`[` `]`, Ctrl+/) still bind it, the rest of the chat waits.
        if let Some(byte) = event_byte(event) {
            match byte {
                b'[' | b']' => {
                    cycle_tile(app, app.home.chat_tile_focused(), byte == b']');
                    return true;
                }
                // `Ctrl+F` then a digit: that layout, and the mode is done.
                b'1'..=b'9' => {
                    let index = usize::from(byte - b'1');
                    if load_layout(app, index) {
                        toggle_home_arrange(app);
                    } else {
                        app.banner = Some(Banner::info(LayoutsError::NoSuchLayout.message()));
                    }
                    return true;
                }
                b'L' => {
                    app.layout_list = Some(LayoutList::open(app.home_layouts.current()));
                    return true;
                }
                // The guide, on the layouts topic: every key swallowed below
                // would take the global `?` with it.
                b'?' => {
                    crate::app::input::open_guide_globally(app);
                    return true;
                }
                _ => {}
            }
            if handle_tile_keys(app, byte) {
                return true;
            }
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

/// Enter or leave Home's arrange mode. Either way the pickers close (the
/// ascii piece picker too, opened from a tile in the mode); leaving writes
/// the layout at once. While the mode is on, Home's bottom row is a footer
/// with every key (`statusline::bar::HOME_ARRANGE_KEYS`).
pub(crate) fn toggle_home_arrange(app: &mut App) {
    let arrange = !app.home.arrange;
    app.layout_list = None;
    app.piece_picker.close();
    app.home.arrange = arrange;
    app.home.close_kind_picker();
    app.home.close_guest_picker();
    if !arrange {
        // Leaving puts the focus back on a chat, the only tiles that keep
        // it outside arrange mode.
        if !app.home.focused_kind().is_some_and(TileKind::takes_focus)
            && let Some(ordinal) = app.home.first_tile_of(TileKind::Chat)
        {
            app.home.focus = ordinal;
            focus_moved(app);
        }
        app.flush_home_layout();
    }
}

/// Show the layout at `index` (`home_layouts`): the page as it is goes
/// into its own slot first, so nothing arranged is lost, then the slot
/// becomes current and the page draws it. False past the end.
pub(crate) fn load_layout(app: &mut App, index: usize) -> bool {
    if index >= app.home_layouts.len() {
        return false;
    }
    app.home_layouts.store_current(app.home.rice.clone());
    app.home_layouts.switch(index);
    show_current_layout(app);
    true
}

/// Draw the current slot on the page: after a switch, a delete of the
/// one that was up, or a reset of it.
fn show_current_layout(app: &mut App) {
    let rice = app.home_layouts.current_layout().clone();
    let stock = app.home_layouts.current_stock();
    app.home
        .load(rice, stock, Instant::now(), &mut rand::thread_rng());
    app.sync_aquarium_bounds();
    focus_moved(app);
    app.mark_home_layout_dirty();
}

/// Save the page as a new layout under `name`, which becomes current; the
/// page keeps showing it, with stock Home as what `R` puts back.
pub(crate) fn save_layout_as(app: &mut App, name: &str) -> Result<usize, LayoutsError> {
    app.home_layouts.store_current(app.home.rice.clone());
    let index = app.home_layouts.save_new(name, app.home.rice.clone())?;
    app.home.set_stock(Layouts::stock_of(name));
    app.mark_home_layout_dirty();
    Ok(index)
}

/// `/layout` from any composer: the list, a layout by name or digit, or
/// the page saved under a name. Every form runs on Home, so another page
/// walks there first.
pub(crate) fn run_layout_command(app: &mut App, command: LayoutCommand) {
    crate::app::input::go_home_for_layouts(app);
    match command {
        LayoutCommand::List => {
            if !app.home.arrange {
                toggle_home_arrange(app);
            }
            app.layout_list = Some(LayoutList::open(app.home_layouts.current()));
        }
        LayoutCommand::Load(target) => {
            let index = match target.parse::<usize>() {
                Ok(digit) if (1..=app.home_layouts.len()).contains(&digit) => Some(digit - 1),
                Ok(_) => None,
                Err(_) => app.home_layouts.index_of(&target),
            };
            match index {
                Some(index) => {
                    load_layout(app, index);
                }
                None => {
                    app.banner = Some(Banner::info(LayoutsError::NoSuchLayout.message()));
                }
            }
        }
        LayoutCommand::Save(name) => match save_layout_as(app, &name) {
            Ok(index) => {
                app.banner = Some(Banner::info(&format!(
                    "Saved as layout {}: Ctrl+F {} brings it back",
                    index + 1,
                    index + 1
                )));
            }
            Err(error) => {
                app.banner = Some(Banner::info(error.message()));
            }
        },
    }
}

/// `/zen` and the status line's care click: Home's layout named `zen`,
/// loaded the way `/layout zen` loads it. A device that renamed or deleted
/// its `zen` layout gets the banner a missing name gets.
pub(crate) fn load_zen_layout(app: &mut App) {
    run_layout_command(
        app,
        LayoutCommand::Load(super::presets::STOCK_ZEN_NAME.to_string()),
    );
}

/// The layouts list: `j` `k` and the arrows move, Enter loads the row and
/// leaves arrange mode, `n` types a name for the page as a new layout,
/// `r` retypes the row's name, `d` deletes the row (never the last), `R`
/// puts the row back to its stock. While a name is being typed the
/// printable keys spell it, Backspace erases, Enter keeps it and Esc
/// drops it (`dispatch_escape`). Everything else is swallowed.
fn handle_layout_list(app: &mut App, event: &ParsedInput) {
    let Some(list) = app.layout_list.as_mut() else {
        return;
    };
    if let Some(edit) = list.editing.as_mut() {
        match (event, event_byte(event)) {
            (_, Some(b'\r' | b'\n')) => confirm_name_edit(app),
            (_, Some(0x7f | 0x08)) => edit.pop(),
            (_, Some(byte)) if byte.is_ascii_graphic() || byte == b' ' => edit.push(byte as char),
            (ParsedInput::Char(ch), None) if !ch.is_control() => edit.push(*ch),
            _ => {}
        }
        return;
    }
    let len = app.home_layouts.len();
    let selected = list.selected;
    match (event, event_byte(event)) {
        (ParsedInput::Arrow(b'A'), _) | (_, Some(b'k')) => list.move_by(-1, len),
        (ParsedInput::Arrow(b'B'), _) | (_, Some(b'j')) => list.move_by(1, len),
        (_, Some(b'\r' | b'\n')) => {
            app.layout_list = None;
            if load_layout(app, selected) {
                toggle_home_arrange(app);
            }
        }
        (_, Some(b'n')) => {
            if app.home_layouts.is_full() {
                app.banner = Some(Banner::info(LayoutsError::Full.message()));
            } else {
                list.editing = Some(NameEdit::new(NamePurpose::SaveNew, String::new()));
            }
        }
        (_, Some(b'r')) => {
            let name = app
                .home_layouts
                .get(selected)
                .map(|item| item.name.clone())
                .unwrap_or_default();
            list.editing = Some(NameEdit::new(NamePurpose::Rename(selected), name));
        }
        (_, Some(b'd')) => {
            let was_current = app.home_layouts.current() == selected;
            match app.home_layouts.delete(selected) {
                Ok(()) => {
                    list.move_by(0, app.home_layouts.len());
                    if was_current {
                        show_current_layout(app);
                    } else {
                        app.mark_home_layout_dirty();
                    }
                }
                Err(error) => {
                    app.banner = Some(Banner::info(error.message()));
                }
            }
        }
        (_, Some(b'R')) => {
            if app.home_layouts.reset(selected) {
                if app.home_layouts.current() == selected {
                    show_current_layout(app);
                } else {
                    app.mark_home_layout_dirty();
                }
            }
        }
        _ => {}
    }
}

/// Enter on a typed name: the page saved under it, or the row renamed.
/// A refused name says why in a banner and the field stays up with the
/// text, so one more key fixes it.
fn confirm_name_edit(app: &mut App) {
    let Some(list) = app.layout_list.as_mut() else {
        return;
    };
    let Some(edit) = list.editing.take() else {
        return;
    };
    let result = match edit.purpose {
        NamePurpose::SaveNew => save_layout_as(app, &edit.text).map(|index| {
            if let Some(list) = app.layout_list.as_mut() {
                list.selected = index;
            }
        }),
        NamePurpose::Rename(index) => {
            let renamed = app.home_layouts.rename(index, &edit.text);
            if renamed.is_ok() {
                app.mark_home_layout_dirty();
            }
            renamed
        }
    };
    if let Err(error) = result {
        app.banner = Some(Banner::info(error.message()));
        if let Some(list) = app.layout_list.as_mut() {
            list.editing = Some(edit);
        }
    }
}

/// The focused tile's own keys in arrange mode, where every tile takes the
/// focus: the attention guest up on it takes `j` `k` and Enter ahead of
/// its host, an Inbox or Headlines tile walks and opens its rows, Enter on
/// an ascii tile picks its piece from the list (`[` `]` step it, with the
/// layout keys), and `a` on the reef or the tank feeds it. None of these
/// collide with the layout keys, which are uppercase or punctuation,
/// `b` `g` `t` aside.
fn handle_tile_keys(app: &mut App, byte: u8) -> bool {
    if app.home.focused_guests().contains(&Guest::Attention) && handle_attention(app, byte) {
        return true;
    }
    match app.home.focused_kind() {
        Some(TileKind::Inbox) => handle_inbox(app, byte),
        Some(TileKind::Headlines) => handle_headlines(app, byte),
        Some(TileKind::Ascii) if matches!(byte, b'\r' | b'\n') => {
            crate::app::ascii::picker::input::open(app);
            true
        }
        Some(TileKind::Aquarium | TileKind::Tank) if byte == b'a' => {
            crate::app::input::feed_aquarium_globally(app);
            true
        }
        Some(_) | None => false,
    }
}

/// `[` `]` on the focused tile: a chat walks its joined rooms, an ascii tile
/// its pieces. Any other tile takes the key and does nothing.
fn cycle_tile(app: &mut App, chat_focused: bool, forward: bool) {
    match chat_focused {
        true => cycle_room(app, if forward { 1 } else { -1 }),
        false => {
            if app
                .home
                .cycle_focused_piece(forward, Instant::now(), &mut rand::thread_rng())
            {
                app.mark_home_layout_dirty();
            }
        }
    }
}

/// The focused Inbox tile: `j` `k` walk its rows, Enter opens the row.
fn handle_inbox(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            let last = inbox_rows(app).len().saturating_sub(1);
            app.home.inbox_selected = (app.home.inbox_selected + 1).min(last);
            true
        }
        b'k' | b'K' => {
            app.home.inbox_selected = app.home.inbox_selected.saturating_sub(1);
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

/// Open the selected Inbox row (`open_inbox_target`): a DM lands in its
/// room, a mention on its message.
fn open_inbox_row(app: &mut App) {
    let rows = inbox_rows(app);
    let Some(row) = rows.get(app.home.inbox_selected.min(rows.len().saturating_sub(1))) else {
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
    open_inbox_target(app, room_id, message_id);
}

/// The attention guest's rows while it is up on the focused tile: `j` `k`
/// move, Enter does the row's act (`AttentionItem::act`): the lobby for
/// a match, the room for a DM or a mention (as an Inbox row opens), the
/// tank's meal, the tree's water, the pet's pat. False with the list
/// empty, since the guest is down and the host has the keys.
fn handle_attention(app: &mut App, byte: u8) -> bool {
    let items = app.attention_items();
    if items.is_empty() {
        return false;
    }
    match byte {
        b'j' | b'J' => {
            let last = items.len() - 1;
            app.home.attention_selected = (app.home.attention_selected + 1).min(last);
            true
        }
        b'k' | b'K' => {
            app.home.attention_selected = app.home.attention_selected.saturating_sub(1);
            true
        }
        b'\r' | b'\n' => {
            let selected = app.home.attention_selected.min(items.len() - 1);
            act_on_attention(app, items[selected].clone());
            true
        }
        _ => false,
    }
}

fn act_on_attention(app: &mut App, item: AttentionItem) {
    match item {
        AttentionItem::YourMove { .. } | AttentionItem::Result { .. } => {
            crate::app::input::open_daily_modal_globally(app);
        }
        AttentionItem::Dm { room_id, .. } => open_inbox_target(app, room_id, None),
        AttentionItem::Mention {
            room_id,
            message_id,
            ..
        } => open_inbox_target(app, room_id, Some(message_id)),
        AttentionItem::FeedTank => crate::app::input::feed_aquarium_globally(app),
        AttentionItem::WaterTree => {
            let _ = app
                .bonsai
                .request(crate::app::bonsai::state::BonsaiAction::Water);
        }
        AttentionItem::PetPet => crate::app::input::pet_the_pet_globally(app),
    }
}

/// Land in `room_id` in the page's first chat tile, on `message_id` when
/// there is one, the way a `Ctrl+/` message jump does. A mention in a
/// room the account never joined has no tile to land in and reads in the
/// history modal instead.
fn open_inbox_target(app: &mut App, room_id: Uuid, message_id: Option<Uuid>) {
    if let Some(message_id) = message_id
        && !app.chat.rooms.iter().any(|(room, _)| room.id == room_id)
    {
        app.chat.open_history_at_message(room_id, message_id);
        return;
    }
    let Some(chat_tile) = app.home.first_tile_of(TileKind::Chat) else {
        app.banner = Some(Banner::info("Add a chat tile to open it here"));
        return;
    };
    app.home.focus = chat_tile;
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
            app.home.headlines_selected = (app.home.headlines_selected + 1).min(last);
            true
        }
        b'k' | b'K' => {
            app.home.headlines_selected = app.home.headlines_selected.saturating_sub(1);
            true
        }
        b'\r' | b'\n' => {
            let rows = headline_rows(app);
            if let Some(row) = rows.get(
                app.home
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
/// (`App::flush_home_layout`).
fn handle_rice(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Arrow(b'D') | ParsedInput::Arrow(b'A') | ParsedInput::BackTab => {
            app.home.focus_prev();
            focus_moved(app);
            return true;
        }
        ParsedInput::Arrow(b'C') | ParsedInput::Arrow(b'B') => {
            app.home.focus_next();
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
            app.home.focus_next();
            focus_moved(app);
            return true;
        }
        b' ' => {
            app.home.open_kind_picker();
            return true;
        }
        b'+' => {
            app.home.open_guest_picker();
            return true;
        }
        b'S' => {
            if app.home.leaf_count() >= MAX_TILES {
                app.banner = Some(Banner::info("That is every tile the page holds"));
                return true;
            }
            let wide = focused_tile_is_wide(app);
            app.home.split_focused(wide)
        }
        b'X' => {
            if app.home.leaf_count() <= 1 {
                app.banner = Some(Banner::info("The last tile stays"));
                return true;
            }
            app.home.close_focused()
        }
        b'<' | b',' => resize_or_explain(app, Dir::Row, -1),
        b'>' | b'.' => resize_or_explain(app, Dir::Row, 1),
        b'{' => resize_or_explain(app, Dir::Column, -1),
        b'}' => resize_or_explain(app, Dir::Column, 1),
        b'F' => app.home.flip_focused(),
        b'b' => {
            app.home.cycle_border();
            true
        }
        b'g' => {
            app.home.cycle_gap();
            true
        }
        // The focused tile's own title rule.
        b't' => app.home.toggle_focused_title(),
        b'R' => {
            app.home.reset();
            true
        }
        _ => return false,
    };
    if changed {
        app.sync_aquarium_bounds();
        // An edit can land the focus, and so the active chat, elsewhere.
        focus_moved(app);
        app.mark_home_layout_dirty();
    }
    true
}

/// The tile picker: `j` `k` and the arrows move, Enter or `space` picks,
/// everything else is swallowed. A refused row (Chat past the cap) says
/// so in a banner and leaves the picker up.
fn handle_kind_picker(app: &mut App, event: &ParsedInput) {
    let byte = event_byte(event);
    match (event, byte) {
        (ParsedInput::Arrow(b'A'), _) | (_, Some(b'k')) => app.home.move_kind_picker(-1),
        (ParsedInput::Arrow(b'B'), _) | (_, Some(b'j')) => app.home.move_kind_picker(1),
        (_, Some(b'\r' | b'\n' | b' ')) => match app.home.pick_kind() {
            KindPick::Changed => {
                app.pin_chat_tiles_on_pick();
                app.sync_aquarium_bounds();
                focus_moved(app);
                app.mark_home_layout_dirty();
            }
            KindPick::Unchanged => {}
            KindPick::ChatFull => {
                app.banner = Some(Banner::info("That is every chat tile the page holds"));
            }
        },
        _ => {}
    }
}

/// The guest picker: `j` `k` and the arrows move, Enter or `space` puts
/// the row's guest on the focused tile (or takes the tile's own off) and
/// keeps the picker up, everything else is swallowed.
fn handle_guest_picker(app: &mut App, event: &ParsedInput) {
    let byte = event_byte(event);
    match (event, byte) {
        (ParsedInput::Arrow(b'A'), _) | (_, Some(b'k')) => app.home.move_guest_picker(-1),
        (ParsedInput::Arrow(b'B'), _) | (_, Some(b'j')) => app.home.move_guest_picker(1),
        (_, Some(b'\r' | b'\n' | b' ')) => match app.home.pick_guest() {
            GuestPick::Changed => {
                app.sync_aquarium_bounds();
                app.mark_home_layout_dirty();
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
    if app.chat.composing && app.chat.composer_room_id() != app.home_chat_room_id() {
        app.chat.reset_composer();
    }
    app.sync_visible_chat_room();
}

/// Move the focused tile's edge by `delta_cells` along `dir`, or say why
/// nothing moved: a tile with no split of that direction above it has
/// nothing to trade.
fn resize_or_explain(app: &mut App, dir: Dir, delta_cells: i16) -> bool {
    let tiles_area = app.home_tiles_area();
    if app.home.resize_focused(dir, delta_cells, tiles_area) {
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
    let tiles_area = app.home_tiles_area();
    let rects = super::layout::tile_rects(&app.home.rice.root, tiles_area, app.home.gaps());
    rects
        .get(app.home.focus)
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
        .home_chat_room_id()
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
    if !app.home.bind_active_chat_room(Some(room_id)) {
        return false;
    }
    app.chat.reset_composer();
    app.chat.clear_message_selection();
    app.sync_rail_to_active_chat();
    app.sync_visible_chat_room();
    app.mark_home_layout_dirty();
    true
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
