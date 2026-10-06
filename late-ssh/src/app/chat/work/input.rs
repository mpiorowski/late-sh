use crate::app::common::primitives::Banner;
use crate::app::directory::editor;
use crate::app::state::App;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.work.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.work.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'i' | b'I' => {
            editor::input::open_own(app, editor::state::Page::Card);
            true
        }
        b'e' | b'E' => {
            let id = app.chat.work.selected_item().map(|item| item.profile.id);
            match id {
                Some(id) if editor::input::open_card(app, id) => {}
                _ => app.banner = Some(Banner::error("not your work card")),
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            let base_url = app.web_url.as_str();
            if let Some(url) = app.chat.work.copy_selected_profile_url(base_url) {
                app.pending_clipboard = Some(url);
                app.banner = Some(Banner::success("Work profile link copied!"));
            }
            true
        }
        b'j' | b'J' => {
            app.chat.work.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.work.move_selection(-1);
            true
        }
        b'd' | b'D' => {
            if let Some(banner) = app.chat.work.delete_selected() {
                app.banner = Some(banner);
            }
            true
        }
        b'/' => {
            app.chat.work.toggle_mine_only();
            let banner = if app.chat.work.mine_only() {
                Banner::success("Showing only your work profile.")
            } else {
                Banner::success("Showing all work profiles.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}
