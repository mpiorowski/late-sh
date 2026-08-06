use crate::app::common::primitives::Banner;
use crate::app::{input::ParsedInput, state::App};

use super::state::{DirectoryEntry, DirectoryEntryId, merged_entries};

/// The selected merged-feed row, resolved to an index into the owning chat
/// state's `all_items()` so actions can delegate through `select_index`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectorySelection {
    Project(usize),
    Person(usize),
}

fn entry_len(app: &App) -> usize {
    merged_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.filter,
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    )
    .len()
}

fn selected_id(app: &App) -> Option<DirectoryEntryId> {
    let entries = merged_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.filter,
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    entries
        .get(app.directory_state.selected())
        .map(DirectoryEntry::id)
}

/// Resolve the merged selection to `(kind, index into that kind's
/// all_items())` plus the author identity, as owned values so the caller can
/// mutate `app` afterwards.
pub(crate) fn resolve_selection(app: &App) -> Option<(DirectorySelection, uuid::Uuid, String)> {
    let entries = merged_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.filter,
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    let entry = entries.get(app.directory_state.selected())?;
    let author = (entry.user_id(), entry.author_username().to_string());
    let selection = match entry {
        DirectoryEntry::Project(item) => {
            let idx = app
                .chat
                .showcase
                .all_items()
                .iter()
                .position(|candidate| candidate.showcase.id == item.showcase.id)?;
            DirectorySelection::Project(idx)
        }
        DirectoryEntry::Person(item) => {
            let idx = app
                .chat
                .work
                .all_items()
                .iter()
                .position(|candidate| candidate.profile.id == item.profile.id)?;
            DirectorySelection::Person(idx)
        }
    };
    Some((selection, author.0, author.1))
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

/// Leaving search keeps the highlighted row highlighted: capture its identity
/// under the query, rebuild the query-less list, and re-find it there.
fn submit_search(app: &mut App) {
    let id = selected_id(app);
    app.directory_state.exit_search();
    let entries = merged_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.filter,
        app.directory_state.mine_only,
        app.user_id,
        "",
    );
    let index = id
        .and_then(|id| entries.iter().position(|entry| entry.id() == id))
        .unwrap_or(0);
    app.directory_state.select(index);
}

/// Idle (not composing, not searching) keys for the merged feed.
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
            if let Some((_, user_id, username)) = resolve_selection(app) {
                app.open_profile_modal(user_id, username);
            }
            true
        }
        b'e' | b'E' => {
            match resolve_selection(app) {
                Some((DirectorySelection::Project(idx), _, _)) => {
                    app.chat.showcase.select_index(idx);
                    if !app.chat.showcase.start_editing_selected() {
                        app.banner = Some(Banner::error("not your showcase"));
                    }
                }
                Some((DirectorySelection::Person(idx), _, _)) => {
                    app.chat.work.select_index(idx);
                    if !app.chat.work.start_editing_selected() {
                        app.banner = Some(Banner::error("not your work profile"));
                    }
                }
                None => {}
            }
            true
        }
        b'd' | b'D' => {
            match resolve_selection(app) {
                Some((DirectorySelection::Project(idx), _, _)) => {
                    app.chat.showcase.select_index(idx);
                    if let Some(banner) = app.chat.showcase.delete_selected() {
                        app.banner = Some(banner);
                    }
                }
                Some((DirectorySelection::Person(idx), _, _)) => {
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
            match resolve_selection(app) {
                Some((DirectorySelection::Project(idx), _, _)) => {
                    app.chat.showcase.select_index(idx);
                    if let Some(url) = app.chat.showcase.copy_selected_url() {
                        app.pending_clipboard = Some(url);
                        app.banner = Some(Banner::success("Project link copied!"));
                    }
                }
                Some((DirectorySelection::Person(idx), _, _)) => {
                    let base_url = app.web_url.clone();
                    app.chat.work.select_index(idx);
                    if let Some(url) = app.chat.work.copy_selected_profile_url(&base_url) {
                        app.pending_clipboard = Some(url);
                        app.banner = Some(Banner::success("Work profile link copied!"));
                    }
                }
                None => {}
            }
            true
        }
        b'/' => {
            app.directory_state.toggle_mine_only();
            let banner = if app.directory_state.mine_only {
                Banner::success("Showing only your entries.")
            } else {
                Banner::success("Showing everyone.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}

/// Idle page-sized selection jumps on the merged feed.
pub(crate) fn move_idle_selection(app: &mut App, delta: isize) {
    let len = entry_len(app);
    app.directory_state.move_selection(delta, len);
}

/// Idle arrow keys: selection moves on the merged feed.
pub(crate) fn handle_idle_arrow(app: &mut App, key: u8) -> bool {
    let len = entry_len(app);
    match key {
        b'A' => {
            app.directory_state.move_selection(-1, len);
            true
        }
        b'B' => {
            app.directory_state.move_selection(1, len);
            true
        }
        _ => false,
    }
}
