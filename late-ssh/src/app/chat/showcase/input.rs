use crate::app::common::primitives::Banner;
use crate::app::state::App;

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        0x1B => app.chat.showcase.stop_composing(),
        b'\t' => app.chat.showcase.cycle_field(true),
        // Ctrl-J inserts a newline only in description. Plain Enter submits.
        b'\n' => app.chat.showcase.field_newline(),
        b'\r' => {
            if let Some(banner) = app.chat.showcase.submit() {
                app.banner = Some(banner);
            }
        }
        // Ctrl-U: clear current field line
        0x15 => app.chat.showcase.field_clear_line(),
        // Ctrl-Y: paste from kill-ring
        0x19 => app.chat.showcase.field_paste(),
        // Ctrl-/ (Ctrl-_): undo
        0x1F => app.chat.showcase.field_undo(),
        0x7F | 0x08 => app.chat.showcase.field_delete_char(),
        b if (32..127).contains(&b) => {
            app.chat.showcase.field_insert_char(b as char);
        }
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.chat.showcase.composing() {
        let field = app.chat.showcase.active_field();
        if matches!(field, super::state::ComposerField::Description) {
            let input = match key {
                b'A' => ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Up,
                    ..Default::default()
                },
                b'B' => ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Down,
                    ..Default::default()
                },
                b'C' => ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Right,
                    ..Default::default()
                },
                b'D' => ratatui_textarea::Input {
                    key: ratatui_textarea::Key::Left,
                    ..Default::default()
                },
                _ => return false,
            };
            app.chat.showcase.field_input(field, input);
            return true;
        }

        // Horizontal arrows move inside single-line fields.
        let input = match key {
            b'C' => ratatui_textarea::Input {
                key: ratatui_textarea::Key::Right,
                ..Default::default()
            },
            b'D' => ratatui_textarea::Input {
                key: ratatui_textarea::Key::Left,
                ..Default::default()
            },
            _ => return false,
        };
        app.chat.showcase.field_input(field, input);
        return true;
    }

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
            app.chat.showcase.start_composing();
            true
        }
        b'e' | b'E' => {
            if !app.chat.showcase.start_editing_selected() {
                app.banner = Some(Banner::error("not your showcase"));
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
        _ => false,
    }
}
