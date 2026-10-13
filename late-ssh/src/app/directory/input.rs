use crate::app::common::primitives::Banner;
use crate::app::directory::editor;
use crate::app::{input::ParsedInput, state::App};

use super::mouse::Target;
use super::state::{PersonFocus, Shelf, person_entries};
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

/// True when the event landed on something this page draws. Anything else
/// (keyboard-only mode, a click on empty space, moves and releases) falls
/// through to the global handlers: the sidebar, the status bar, the frame.
pub(crate) fn handle_mouse(app: &mut App, mouse: MouseEvent) -> bool {
    if !app.interaction_mode.mouse_enabled() {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let size = app.size;
    match mouse.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let delta = if mouse.kind == MouseEventKind::ScrollUp {
                -3
            } else {
                3
            };
            if app.directory_state.shelf() == Shelf::Jobs {
                app.jobs.mouse.scroll(x, y, delta, size)
            } else {
                app.directory_state.mouse.scroll(x, y, delta, size)
            }
        }
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
            let on_jobs = app.directory_state.shelf() == Shelf::Jobs;
            if app.directory_state.mouse.click_track(x, y, size)
                || (on_jobs && app.jobs.mouse.click_track(x, y, size))
            {
                return true;
            }
            let target = app
                .directory_state
                .mouse
                .target(x, y, size)
                .or_else(|| on_jobs.then(|| app.jobs.mouse.target(x, y, size)).flatten());
            let Some(target) = target else {
                return false;
            };
            match target {
                Target::Shelf(shelf) => {
                    app.directory_state.set_shelf(shelf);
                    app.jobs.mouse.invalidate();
                }
                Target::Person(id) => {
                    let entries = person_entries(
                        app.chat.showcase.all_items(),
                        app.chat.work.all_items(),
                        app.directory_state.mine_only,
                        app.user_id,
                        app.directory_state.active_query(),
                    );
                    if let Some(index) = entries.iter().position(|entry| entry.user_id == id) {
                        app.directory_state.select_and_open(index);
                        if app.directory_state.search_mode() {
                            submit_search(app);
                        }
                    }
                }
                Target::Job(id) => {
                    let tags = crate::app::jobs::input::own_tags(app);
                    if let Some(index) = app.jobs.visible(&tags).iter().position(|job| job.id == id)
                    {
                        app.directory_state.set_shelf(Shelf::Jobs);
                        app.jobs.select_and_open(index);
                    }
                }
                Target::Key(key) => {
                    handle_idle_byte(app, key);
                }
                Target::Item(item, key) => {
                    if focus_item(app, item) && key != 0 {
                        handle_people_byte(app, key);
                    }
                }
                Target::Copy(url) => {
                    app.pending_clipboard = Some(url);
                    app.banner = Some(Banner::success("Link copied!"));
                }
                Target::Profile(id, name) => app.open_profile_modal(id, name),
                Target::Back => {
                    if on_jobs {
                        app.jobs.close_detail();
                    } else {
                        app.directory_state.close_detail();
                    }
                }
            }
            app.directory_state.mouse.invalidate();
            app.jobs.mouse.invalidate();
            true
        }
        _ => false,
    }
}

fn focus_item(app: &mut App, wanted: FocusedItem) -> bool {
    let entries = person_entries(
        app.chat.showcase.all_items(),
        app.chat.work.all_items(),
        app.directory_state.mine_only,
        app.user_id,
        app.directory_state.active_query(),
    );
    let found = entries
        .get(app.directory_state.selected())
        .and_then(|entry| {
            (0..entry.focus_len()).find(|index| match entry.focus_target(*index) {
                Some(PersonFocus::Card(item)) => wanted == FocusedItem::Card(item.profile.id),
                Some(PersonFocus::Project(item)) => {
                    wanted == FocusedItem::Project(item.showcase.id)
                }
                None => false,
            })
        });
    if let Some(index) = found {
        app.directory_state.focus_item(index);
        true
    } else {
        false
    }
}

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

fn handle_jobs_byte(app: &mut App, byte: u8) -> bool {
    crate::app::jobs::input::handle_byte(app, byte)
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
    match app.directory_state.shelf() {
        Shelf::Jobs => crate::app::jobs::input::move_selection(app, delta),
        Shelf::People => {
            let len = entry_len(app);
            app.directory_state.move_selection(delta, len);
        }
    }
}

/// Idle arrow keys: up/down move between people, left/right move the detail
/// focus across the selected person's card and projects (or, stacked, open
/// and close the detail pane).
pub(crate) fn handle_idle_arrow(app: &mut App, key: u8) -> bool {
    if app.directory_state.shelf() == Shelf::Jobs {
        return crate::app::jobs::input::handle_arrow(app, key);
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
    if app.directory_state.shelf() == Shelf::Jobs {
        return crate::app::jobs::input::handle_escape(app);
    }
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
