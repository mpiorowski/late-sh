use crate::app::common::primitives::Banner;
use crate::app::{input::ParsedInput, state::App};

use super::state::{PersonFocus, person_entries};

/// The focused item of the selected person, resolved to an index into the
/// owning chat state's `all_items()` so actions can delegate through
/// `select_index`. Owned values, so the caller can mutate `app` afterwards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FocusedItem {
    Card(usize),
    Project(usize),
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
        PersonFocus::Card(item) => {
            let idx = app
                .chat
                .work
                .all_items()
                .iter()
                .position(|candidate| candidate.profile.id == item.profile.id)?;
            FocusedItem::Card(idx)
        }
        PersonFocus::Project(item) => {
            let idx = app
                .chat
                .showcase
                .all_items()
                .iter()
                .position(|candidate| candidate.showcase.id == item.showcase.id)?;
            FocusedItem::Project(idx)
        }
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
    app.directory_state.select(index);
}

/// Idle (not composing, not searching) keys for the people feed.
pub(crate) fn handle_idle_byte(app: &mut App, byte: u8) -> bool {
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
            let len = focus_len(app);
            app.directory_state.move_focus(-1, len);
            true
        }
        b'l' | b'L' => {
            let len = focus_len(app);
            app.directory_state.move_focus(1, len);
            true
        }
        b'i' | b'I' => {
            app.chat.showcase.start_composing();
            true
        }
        b'w' | b'W' => {
            app.chat.work.start_composing();
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
            match resolve_selection(app).map(|selection| selection.focused) {
                Some(FocusedItem::Project(idx)) => {
                    app.chat.showcase.select_index(idx);
                    if !app.chat.showcase.start_editing_selected() {
                        app.banner = Some(Banner::error("not your project"));
                    }
                }
                Some(FocusedItem::Card(idx)) => {
                    app.chat.work.select_index(idx);
                    if !app.chat.work.start_editing_selected() {
                        app.banner = Some(Banner::error("not your work card"));
                    }
                }
                None => {}
            }
            true
        }
        b'd' | b'D' => {
            match resolve_selection(app).map(|selection| selection.focused) {
                Some(FocusedItem::Project(idx)) => {
                    app.chat.showcase.select_index(idx);
                    if let Some(banner) = app.chat.showcase.delete_selected() {
                        app.banner = Some(banner);
                    }
                }
                Some(FocusedItem::Card(idx)) => {
                    app.chat.work.select_index(idx);
                    if let Some(banner) = app.chat.work.delete_selected() {
                        app.banner = Some(banner);
                    }
                }
                None => {}
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            match resolve_selection(app).map(|selection| selection.focused) {
                Some(FocusedItem::Project(idx)) => {
                    app.chat.showcase.select_index(idx);
                    if let Some(url) = app.chat.showcase.copy_selected_url() {
                        app.pending_clipboard = Some(url);
                        app.banner = Some(Banner::success("Project link copied!"));
                    }
                }
                Some(FocusedItem::Card(idx)) => {
                    let base_url = app.web_url.clone();
                    app.chat.work.select_index(idx);
                    if let Some(url) = app.chat.work.copy_selected_profile_url(&base_url) {
                        app.pending_clipboard = Some(url);
                        app.banner = Some(Banner::success("Profile link copied!"));
                    }
                }
                None => {}
            }
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

/// Idle page-sized selection jumps on the people feed.
pub(crate) fn move_idle_selection(app: &mut App, delta: isize) {
    let len = entry_len(app);
    app.directory_state.move_selection(delta, len);
}

/// Idle arrow keys: up/down move between people, left/right move the detail
/// focus across the selected person's card and projects.
pub(crate) fn handle_idle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            let len = entry_len(app);
            app.directory_state.move_selection(-1, len);
            true
        }
        b'B' => {
            let len = entry_len(app);
            app.directory_state.move_selection(1, len);
            true
        }
        b'D' => {
            let len = focus_len(app);
            app.directory_state.move_focus(-1, len);
            true
        }
        b'C' => {
            let len = focus_len(app);
            app.directory_state.move_focus(1, len);
            true
        }
        _ => false,
    }
}
