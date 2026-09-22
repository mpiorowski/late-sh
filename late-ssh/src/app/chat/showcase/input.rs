use crate::app::common::primitives::Banner;
use crate::app::directory::editor;
use crate::app::state::App;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.showcase.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.showcase.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'i' | b'I' => {
            editor::input::open_own_new_project(app);
            true
        }
        b'e' | b'E' => {
            let id = app
                .chat
                .showcase
                .selected_item()
                .map(|item| item.showcase.id);
            match id {
                Some(id) if editor::input::open_project(app, id) => {}
                _ => app.banner = Some(Banner::error("not your project")),
            }
            true
        }
        b'\r' | b'\n' => {
            if let Some(url) = app.chat.showcase.copy_selected_url() {
                let cleaned = crate::app::input::sanitize_paste_markers(&url);
                app.pending_clipboard = Some(cleaned.trim().to_owned());
                app.banner = Some(Banner::success("Link copied!"));
            }
            true
        }
        b'j' | b'J' => {
            app.chat.showcase.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.showcase.move_selection(-1);
            true
        }
        b'd' | b'D' => {
            if let Some(banner) = app.chat.showcase.delete_selected() {
                app.banner = Some(banner);
            }
            true
        }
        b'/' => {
            app.chat.showcase.toggle_mine_only();
            let banner = if app.chat.showcase.mine_only() {
                Banner::success("Showing only your projects.")
            } else {
                Banner::success("Showing all projects.")
            };
            app.banner = Some(banner);
            true
        }
        _ => false,
    }
}
