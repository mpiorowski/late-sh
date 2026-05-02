use crate::app::common::primitives::Banner;
use crate::app::state::App;

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        0x1B => app.chat.work.stop_composing(),
        b'\t' => app.chat.work.cycle_field(true),
        b'\n' => app.chat.work.field_newline(),
        b'\r' => {
            if let Some(banner) = app.chat.work.submit() {
                app.banner = Some(banner);
            }
        }
        0x15 => app.chat.work.field_clear_line(),
        0x19 => app.chat.work.field_paste(),
        0x1F => app.chat.work.field_undo(),
        0x7F | 0x08 => app.chat.work.field_delete_char(),
        b if (32..127).contains(&b) => {
            app.chat.work.field_insert_char(b as char);
        }
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.chat.work.composing() {
        let field = app.chat.work.active_field();
        if matches!(field, super::state::ComposerField::Summary) {
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
            app.chat.work.field_input(field, input);
            return true;
        }

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
        app.chat.work.field_input(field, input);
        return true;
    }

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
            app.chat.work.start_composing();
            true
        }
        b'e' | b'E' => {
            if !app.chat.work.start_editing_selected() {
                app.banner = Some(Banner::error("not your work profile"));
            }
            true
        }
        b'\r' | b'\n' | b'c' | b'C' => {
            if let Some(summary) = app.chat.work.copy_selected_summary() {
                app.pending_clipboard = Some(summary);
                app.banner = Some(Banner::success("Work profile copied!"));
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
        _ => false,
    }
}
