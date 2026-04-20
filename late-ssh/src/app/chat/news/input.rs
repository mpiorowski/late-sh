use crate::app::state::App;

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
            if let Some(url) = app.chat.news.selected_url() {
                let cleaned = crate::app::input::sanitize_paste_markers(url);
                app.pending_clipboard = Some(cleaned.trim().to_owned());
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Link copied!",
                ));
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
        _ => false,
    }
}
