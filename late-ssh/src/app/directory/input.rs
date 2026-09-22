use crate::app::common::primitives::Banner;
use crate::app::directory::editor;
use crate::app::{input::ParsedInput, state::App};

use super::state::{PersonFocus, Shelf, person_entries};

/// The focused item of the selected person: their card or one of their
/// projects, by id. Owned values, so the caller can mutate `app` afterwards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FocusedItem {
    Card(uuid::Uuid),
    Project(uuid::Uuid),
}

pub(crate) struct Selection {
    pub(crate) focused: FocusedItem,
    pub(crate) user_id: uuid::Uuid,
    pub(crate) username: String,
}

fn entry_len(app: &App) -> usize {
    person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    )
    .len()
}

fn focus_len(app: &App) -> usize {
    let entries = person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    entries
        .get(app.directory_state.selected())
        .map(|entry| entry.focus_len())
        .unwrap_or(0)
}

fn selected_user_id(app: &App) -> Option<uuid::Uuid> {
    let entries = person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    entries
        .get(app.directory_state.selected())
        .map(|entry| entry.user_id)
}

/// Resolve the selected person plus the focused item under the detail
/// cursor (their work card, or one of their projects).
pub(crate) fn resolve_selection(app: &App) -> Option<Selection> {
    let entries = person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    let entry = entries.get(app.directory_state.selected())?;
    let user_id = entry.user_id;
    let username = entry.username.to_string();
    let focus = app
        .directory_state
        .focus()
        .min(entry.focus_len().saturating_sub(1));
    let focused = match entry.focus_target(focus)? {
        PersonFocus::Card(item) => FocusedItem::Card(item.profile.id),
        PersonFocus::Project(item) => FocusedItem::Project(item.showcase.id),
    };
    Some(Selection {
        focused,
        user_id,
        username,
    })
}

pub(crate) fn handle_search_input(app: &mut App, event: &ParsedInput) -> bool {
    let len = entry_len(app);
    app.directory_state.clamp_selection(len);

    match event {
        ParsedInput::Byte(0x1B) => {
            app.directory_state.exit_search();
            app.directory_state.clamp_selection(entry_len(app));
        }
        ParsedInput::Byte(b'\r') => submit_search(app),
        ParsedInput::Byte(0x7F | 0x08) => app.directory_state.search_backspace(),
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(0x0A) => {
            app.directory_state.move_selection(1, len);
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(0x0B) => {
            app.directory_state.move_selection(-1, len);
        }
        ParsedInput::PageDown => app.directory_state.move_selection(8, len),
        ParsedInput::PageUp => app.directory_state.move_selection(-8, len),
        ParsedInput::Char(ch) => app.directory_state.search_push(*ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || *byte == b' ' => {
            app.directory_state.search_push(*byte as char);
        }
        _ => {}
    }

    let len = entry_len(app);
    app.directory_state.clamp_selection(len);
    true
}

/// Leaving search keeps the highlighted person highlighted: capture their id
/// under the query, rebuild the query-less list, and re-find them there.
fn submit_search(app: &mut App) {
    let user_id = selected_user_id(app);
    app.directory_state.exit_search();
    let entries = person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        "",
    );
    let index = user_id
        .and_then(|user_id| entries.iter().position(|entry| entry.user_id == user_id))
        .unwrap_or(0);
    app.directory_state.select_and_open(index);
}

/// Keys that work on either shelf.
fn handle_shelf_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b' ' => {
            app.directory_state.toggle_shelf();
            true
        }
        b'w' | b'W' => {
            editor::input::open_own(app, editor::state::Page::Card);
            true
        }
        b'i' | b'I' => {
            editor::input::open_own_new_project(app);
            true
        }
        _ => false,
    }
}

/// Idle (not searching) keys for the page.
pub(crate) fn handle_idle_byte(app: &mut App, byte: u8) -> bool {
    if handle_shelf_byte(app, byte) {
        return true;
    }
    match app.directory_state.shelf() {
        Shelf::Jobs => handle_jobs_byte(app, byte),
        Shelf::People => handle_people_byte(app, byte),
    }
}

/// The Jobs shelf has nothing to select yet; `/` says why.
fn handle_jobs_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'/' => {
            app.banner = Some(Banner::info(
                "Job matches arrive with the job feed; fill your card with w meanwhile.",
            ));
            true
        }
        _ => false,
    }
}

fn handle_people_byte(app: &mut App, byte: u8) -> bool {
    let narrow = app.directory_state.narrow();
    let detail = !narrow || app.directory_state.detail_open();
    match byte {
        b'j' | b'J' => {
            let len = entry_len(app);
            app.directory_state.move_selection(1, len);
            true
        }
        b'k' | b'K' => {
            let len = entry_len(app);
            app.directory_state.move_selection(-1, len);
            true
        }
        b'h' | b'H' => {
            if narrow && app.directory_state.detail_open() {
                app.directory_state.close_detail();
            } else {
                let len = focus_len(app);
                app.directory_state.move_focus(-1, len);
            }
            true
        }
        b'l' | b'L' => {
            if narrow && !app.directory_state.detail_open() {
                app.directory_state.open_detail();
            } else {
                let len = focus_len(app);
                app.directory_state.move_focus(1, len);
            }
            true
        }
        b's' | b'S' => {
            app.directory_state.enter_search();
            true
        }
        b'o' | b'O' => {
            if let Some(selection) = resolve_selection(app) {
                app.open_profile_modal(selection.user_id, selection.username);
            }
            true
        }
        b'e' | b'E' => {
            let opened = match resolve_selection(app).map(|selection| selection.focused) {
                Some(FocusedItem::Project(id)) => Some(editor::input::open_project(app, id)),
                Some(FocusedItem::Card(id)) => Some(editor::input::open_card(app, id)),
                None => None,
            };
            if opened == Some(false) {
                app.banner = Some(Banner::error("not yours to edit"));
            }
            true
        }
        b'd' | b'D' => {
            let banner = match resolve_selection(app).map(|selection| selection.focused) {
                Some(FocusedItem::Project(id)) => app.chat.showcase.delete_project(id),
                Some(FocusedItem::Card(id)) => app.chat.work.delete_card(id),
                None => None,
            };
            if let Some(banner) = banner {
                app.banner = Some(banner);
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            if !detail {
                app.directory_state.open_detail();
                return true;
            }
            copy_focused_link(app);
            true
        }
        b'/' => {
            app.directory_state.toggle_mine_only();
            let banner = if app.directory_state.mine_only {
                Banner::success("Showing only you.")
            } else {
                Banner::success("Showing everyone.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}

/// Enter on the focused item: the project's URL, or the card's public page.
fn copy_focused_link(app: &mut App) {
    match resolve_selection(app).map(|selection| selection.focused) {
        Some(FocusedItem::Project(id)) => {
            if let Some(item) = app.chat.showcase.project(id) {
                let url = item.showcase.url.clone();
                app.pending_clipboard = Some(url);
                app.banner = Some(Banner::success("Project link copied!"));
            }
        }
        Some(FocusedItem::Card(id)) => {
            if let Some(item) = app.chat.work.card(id) {
                let url =
                    super::super::chat::work::state::profile_url(&app.web_url, &item.profile.slug);
                app.pending_clipboard = Some(url);
                app.banner = Some(Banner::success("Profile link copied!"));
            }
        }
        None => {}
    }
}

/// Idle page-sized selection jumps on the people feed.
pub(crate) fn move_idle_selection(app: &mut App, delta: isize) {
    if app.directory_state.shelf() != Shelf::People {
        return;
    }
    let len = entry_len(app);
    app.directory_state.move_selection(delta, len);
}

/// Idle arrow keys: up/down move between people, left/right move the detail
/// focus across the selected person's card and projects (or, stacked, open
/// and close the detail pane).
pub(crate) fn handle_idle_arrow(app: &mut App, key: u8) -> bool {
    if app.directory_state.shelf() != Shelf::People {
        return false;
    }
    match key {
        b'A' => handle_people_byte(app, b'k'),
        b'B' => handle_people_byte(app, b'j'),
        b'D' => handle_people_byte(app, b'h'),
        b'C' => handle_people_byte(app, b'l'),
        _ => false,
    }
}

/// Esc on the page: leave search, or close the stacked detail pane.
pub(crate) fn handle_escape(app: &mut App) -> bool {
    if app.directory_state.search_mode() {
        app.directory_state.exit_search();
        return true;
    }
    if app.directory_state.narrow() && app.directory_state.detail_open() {
        app.directory_state.close_detail();
        return true;
    }
    false
}
