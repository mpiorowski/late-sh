use crate::app::{state::App, vote};

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.select_dashboard_message(1);
            true
        }
        b'B' => {
            app.chat.select_dashboard_message(-1);
            true
        }
        _ => false,
    }
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if vote::input::handle_key(app, byte) {
        return true;
    }

    match byte {
        b'd' | b'D' => {
            if let Some(b) = app.chat.delete_selected_message() {
                app.banner = Some(b);
            }
            return true;
        }
        b'r' | b'R' => {
            app.chat.select_general_room();
            app.chat.begin_reply_to_selected();
            app.chat.clear_message_selection();
            return true;
        }
        _ => {}
    }

    if !matches!(byte, b'j' | b'J' | b'k' | b'K' | 0x04 | 0x15) {
        app.chat.clear_message_selection();
    }

    match byte {
        b'j' | b'J' => {
            app.chat.select_dashboard_message(-1);
            true
        }
        b'k' | b'K' => {
            app.chat.select_dashboard_message(1);
            true
        }
        0x04 => {
            // Ctrl-D: half-page down. `select_dashboard_message` delta is in
            // MESSAGES, not rows; dividing by 6 approximates half a visible
            // page given wrapped messages ~3 rows tall.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_dashboard_message(-step);
            true
        }
        0x15 => {
            // Ctrl-U: half-page up. Same rationale as Ctrl-D above.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_dashboard_message(step);
            true
        }
        b'g' | b'G' => {
            app.chat.clear_message_selection();
            true
        }
        b'i' | b'I' => {
            app.chat.select_general_room();
            app.chat.start_composing();
            true
        }
        b'\r' | b'\n' => {
            app.pending_clipboard =
                Some("curl -fsSL https://cli.late.sh/install.sh | bash".to_string());
            app.banner = Some(crate::app::common::primitives::Banner::success(
                "CLI install command copied!",
            ));
            true
        }
        _ => false,
    }
}
