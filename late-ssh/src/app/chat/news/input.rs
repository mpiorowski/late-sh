use uuid::Uuid;

use crate::app::video::svc::Watched;
use crate::app::{chat::state::RoomSlot, common::primitives::Banner, state::App};

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        // Escape cancels composing and aborts any in-flight URL task.
        0x1B => app.chat.news.stop_composing(),
        b'\r' | b'\n' => app.chat.news.submit_composer(),
        0x15 => {
            // Ctrl-U: clear composer
            app.chat.news.composer_clear();
        }
        0x19 => {
            // Ctrl-Y: yank from kill-ring
            app.chat.news.composer_paste();
        }
        0x1F => {
            // Ctrl-/ (same byte as Ctrl-_): undo
            app.chat.news.composer_undo();
        }
        0x7F | 0x08 => app.chat.news.composer_pop(),
        b if (32..127).contains(&b) => {
            app.chat.news.composer_push(b as char);
        }
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.news.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.news.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'i' | b'I' => {
            app.chat.news.start_composing();
            true
        }
        b'\r' | b'\n' => {
            if let Some(article_id) = app.chat.news.selected_item().map(|item| item.article.id) {
                app.chat.open_reader_for_article(article_id);
            }
            true
        }
        // Watch a video story in the terminal, or learn what that takes.
        // Anything else is not a video, and `w` passes through untouched.
        b'w' | b'W' => {
            let Some(article_id) = app.chat.news.selected_item().map(|item| item.article.id) else {
                return false;
            };
            watch_article(app, article_id)
        }
        b'c' | b'C' => {
            if let Some(url) = app.chat.news.selected_url() {
                let cleaned = crate::app::input::sanitize_paste_markers(url);
                app.pending_clipboard = Some(cleaned.trim().to_owned());
                app.banner = Some(Banner::success("Link copied!"));
            }
            true
        }
        b'j' | b'J' => {
            app.chat.news.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.news.move_selection(-1);
            true
        }
        b'd' | b'D' => {
            app.chat.news.delete_selected();
            true
        }
        // Keep the selected story for later, or let it go: it changes
        // section at once, the cursor stays put.
        b's' | b'S' => {
            if let Some(outcome) = app.chat.news.toggle_save_selected() {
                app.banner = Some(super::state::save_banner(outcome));
            }
            true
        }
        // The live strip's `r`, from the feed: reply in #lounge quoting the
        // selected story, and take the user there to write it.
        b'r' | b'R' => {
            let Some(article_id) = app.chat.news.selected_item().map(|item| item.article.id) else {
                return true;
            };
            reply_in_lounge(app, article_id);
            true
        }
        b'/' => {
            app.chat.news.toggle_mine_only();
            let banner = if app.chat.news.mine_only() {
                crate::app::common::primitives::Banner::success("Showing only your news.")
            } else {
                crate::app::common::primitives::Banner::success("Showing all news.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}

/// Reply in #lounge quoting the article, with #lounge selected so the
/// composer is where the user lands. The News feed's `r` and the reader's
/// (`reader/input.rs`) both come through here. Returns whether the
/// reply started in #lounge.
pub fn reply_in_lounge(app: &mut App, article_id: Uuid) -> bool {
    if !app.chat.begin_reply_to_article(article_id) {
        return false;
    }
    match app.chat.lounge_room_id() {
        Some(lounge_id) => {
            app.chat.select_room_slot(RoomSlot::Room(lounge_id));
            app.sync_visible_chat_room();
            true
        }
        None => false,
    }
}

/// `w` on a video story (the News room or the reader): the watch player
/// (`app/video/`), or the setup box naming what this session is missing.
/// Watching a story reads it; learning what watching takes does not.
/// False when the story is gone or not a video.
pub(crate) fn watch_article(app: &mut App, article_id: Uuid) -> bool {
    let Some(request) = app.chat.news.watch_request(article_id) else {
        return false;
    };
    match crate::app::video::svc::watch(app, request) {
        Watched::Player => app.chat.news.mark_article_read(article_id),
        Watched::Setup => {}
    }
    true
}

/// `o` on the live strip or a Live row: a video this session can play goes
/// straight to the player; anything else opens the reader. `o` is "open",
/// not "watch", so it never lands on the setup box.
pub(crate) fn open_article(app: &mut App, article_id: Uuid) -> bool {
    let request = match crate::app::video::svc::can_play(app) {
        true => app.chat.news.watch_request(article_id),
        false => None,
    };
    match request {
        Some(request) => {
            crate::app::video::svc::play(app, request);
            app.chat.news.mark_article_read(article_id);
            true
        }
        None => app.chat.open_reader_for_article(article_id),
    }
}
