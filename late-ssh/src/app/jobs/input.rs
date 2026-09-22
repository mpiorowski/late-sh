//! Keys on the Jobs shelf. The page-level keys (`Space`, `w`, `i`) are
//! the directory's; these are the shelf's own.

use crate::app::common::primitives::Banner;
use crate::app::state::App;

use super::state::viewer_tags;

/// The viewer's match tags from the session's own data: their card in the
/// work feed, their languages on the profile.
pub(crate) fn own_tags(app: &App) -> Vec<String> {
    let card = app
        .chat
        .work
        .all_items()
        .iter()
        .find(|item| item.profile.user_id == app.user_id)
        .map(|item| &item.profile);
    viewer_tags(card, &app.profile_state.profile().langs)
}

fn visible_len(app: &App) -> usize {
    app.jobs.visible(&own_tags(app)).len()
}

pub(crate) fn handle_byte(app: &mut App, byte: u8) -> bool {
    let narrow = app.jobs.narrow();
    match byte {
        b'j' | b'J' => {
            let len = visible_len(app);
            app.jobs.move_selection(1, len);
            true
        }
        b'k' | b'K' => {
            let len = visible_len(app);
            app.jobs.move_selection(-1, len);
            true
        }
        b'h' | b'H' => {
            if narrow && app.jobs.detail_open() {
                app.jobs.close_detail();
            }
            true
        }
        b'l' | b'L' => {
            if narrow && !app.jobs.detail_open() {
                app.jobs.open_detail();
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            if narrow && !app.jobs.detail_open() {
                app.jobs.open_detail();
                return true;
            }
            copy_link(app);
            true
        }
        b'/' => {
            app.jobs.toggle_for_me();
            let banner = if app.jobs.for_me {
                if own_tags(app).is_empty() {
                    Banner::info(
                        "Nothing to match on yet: put skills on your card (w) or langs on your late.fetch.",
                    )
                } else {
                    Banner::success("Showing postings that carry your tags.")
                }
            } else {
                Banner::success("Showing every posting.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}

fn copy_link(app: &mut App) {
    let tags = own_tags(app);
    let url = app
        .jobs
        .visible(&tags)
        .get(app.jobs.selected())
        .map(|posting| posting.url.clone());
    if let Some(url) = url {
        app.pending_clipboard = Some(url);
        app.banner = Some(Banner::success("Posting link copied!"));
    }
}

pub(crate) fn move_selection(app: &mut App, delta: isize) {
    let len = visible_len(app);
    app.jobs.move_selection(delta, len);
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => handle_byte(app, b'k'),
        b'B' => handle_byte(app, b'j'),
        b'D' => handle_byte(app, b'h'),
        b'C' => handle_byte(app, b'l'),
        _ => false,
    }
}

/// Esc on the shelf: close the stacked detail pane.
pub(crate) fn handle_escape(app: &mut App) -> bool {
    if app.jobs.narrow() && app.jobs.detail_open() {
        app.jobs.close_detail();
        return true;
    }
    false
}
