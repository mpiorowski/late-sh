use crate::app::state::App;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.discover.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.discover.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    // While the filter is open every keystroke edits the query, except the few
    // control keys that navigate/confirm/close it. Arrow keys still route
    // through `handle_arrow`, so j/k are free to be typed into the query.
    if app.chat.discover.is_filtering() {
        match byte {
            0x1B => {
                // Esc is normally routed via dispatch_escape, but handle it here
                // too so it cancels the filter rather than leaving the screen.
                app.chat.discover.cancel_filter();
            }
            b'\r' | b'\n' => {
                if let Some(banner) = app.chat.join_selected_discover_room() {
                    app.banner = Some(banner);
                }
            }
            0x15 => app.chat.discover.clear_query(), // Ctrl-U
            0x7F | 0x08 => app.chat.discover.backspace(),
            b if (32..127).contains(&b) => app.chat.discover.push_char(b as char),
            _ => {}
        }
        return true;
    }

    match byte {
        b'/' => {
            app.chat.discover.start_filter();
            true
        }
        b'j' | b'J' => {
            app.chat.discover.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.discover.move_selection(-1);
            true
        }
        b'\r' | b'\n' => {
            if let Some(banner) = app.chat.join_selected_discover_room() {
                app.banner = Some(banner);
            }
            true
        }
        _ => false,
    }
}
