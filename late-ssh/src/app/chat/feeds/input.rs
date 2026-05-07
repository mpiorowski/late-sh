use crate::app::{common::primitives::Banner, state::App};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.feeds.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.feeds.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'j' | b'J' => {
            app.chat.feeds.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.feeds.move_selection(-1);
            true
        }
        b's' | b'S' => {
            if let Some(banner) = app.chat.feeds.share_selected() {
                app.banner = Some(banner);
            } else {
                app.banner = Some(Banner::error("No feed entry selected."));
            }
            true
        }
        b'r' | b'R' => {
            app.chat.feeds.poll_now();
            app.banner = Some(Banner::success("Refreshing feeds..."));
            true
        }
        b'd' | b'D' => {
            if let Some(banner) = app.chat.feeds.dismiss_selected() {
                app.banner = Some(banner);
            }
            true
        }
        b'\r' | b'\n' => {
            if let Some(url) = app.chat.feeds.selected_url() {
                let cleaned = crate::app::input::sanitize_paste_markers(url);
                app.pending_clipboard = Some(cleaned.trim().to_owned());
                app.banner = Some(Banner::success("Link copied!"));
            }
            true
        }
        _ => false,
    }
}
