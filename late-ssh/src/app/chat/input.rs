use crate::app::state::App;

pub fn handle_compose_input(app: &mut App, byte: u8) {
    if app.chat.is_autocomplete_active() {
        match byte {
            0x1B => {
                app.chat.ac_dismiss();
                return;
            }
            b'\t' | b'\r' | b'\n' => {
                app.chat.ac_confirm();
                return;
            }
            _ => {} // fall through to normal handling
        }
    }

    match byte {
        0x1B => {
            app.chat.stop_composing();
        }
        b'\r' | b'\n' => {
            if let Some(b) = app.chat.submit_composer() {
                app.banner = Some(b);
            }
        }
        0x7F => {
            app.chat.composer_backspace();
            app.chat.update_autocomplete();
        }
        b if (32..127).contains(&b) => {
            app.chat.composer_push(b as char);
            app.chat.update_autocomplete();
        }
        _ => {}
    }
}

pub fn handle_autocomplete_arrow(app: &mut App, key: u8) {
    match key {
        b'A' => app.chat.ac_move_selection(-1),
        b'B' => app.chat.ac_move_selection(1),
        _ => {}
    }
}

pub fn handle_scroll(app: &mut App, delta: isize) {
    app.chat.select_message(delta);
}

fn switch_room(app: &mut App, delta: isize) {
    if app.chat.move_selection(delta) {
        app.chat.mark_selected_room_read();
        app.chat.request_list();
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.chat.notifications_selected {
        return super::notifications::input::handle_arrow(app, key);
    }
    if app.chat.news_selected {
        return super::news::input::handle_arrow(app, key);
    }
    match key {
        b'A' => {
            app.chat.select_message(1);
            true
        }
        b'B' => {
            app.chat.select_message(-1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    if app.chat.notifications_selected {
        match byte {
            b'h' | b'H' => {
                switch_room(app, -1);
                return true;
            }
            b'l' | b'L' => {
                switch_room(app, 1);
                return true;
            }
            _ => return super::notifications::input::handle_byte(app, byte),
        }
    }

    if app.chat.news_selected {
        // h/l still switch rooms even when news is selected
        match byte {
            b'h' | b'H' => {
                switch_room(app, -1);
                return true;
            }
            b'l' | b'L' => {
                switch_room(app, 1);
                return true;
            }
            _ => return super::news::input::handle_byte(app, byte),
        }
    }

    // `d` deletes and keeps the cursor on the adjacent message so you can
    // reap a run of your own messages with repeated presses. `r` enters
    // reply mode and drops the selection.
    match byte {
        b'd' | b'D' => {
            if let Some(b) = app.chat.delete_selected_message() {
                app.banner = Some(b);
            }
            return true;
        }
        b'r' | b'R' => {
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
            app.chat.select_message(-1);
            true
        }
        b'k' | b'K' => {
            app.chat.select_message(1);
            true
        }
        b'h' | b'H' => {
            switch_room(app, -1);
            true
        }
        b'l' | b'L' => {
            switch_room(app, 1);
            true
        }
        b'i' | b'I' | b'\r' | b'\n' => {
            app.chat.start_composing();
            true
        }
        0x04 => {
            // Ctrl-D: half-page down. `select_message` delta is in MESSAGES,
            // not rows, and chat messages wrap to ~3 rows each, so divide
            // terminal height by 6 to feel like half a visible page.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_message(-step);
            true
        }
        0x15 => {
            // Ctrl-U: half-page up. Same rationale as Ctrl-D above.
            let step = (app.size.1 / 6).max(1) as isize;
            app.chat.select_message(step);
            true
        }
        b'g' | b'G' => {
            app.chat.clear_message_selection();
            true
        }
        b'c' | b'C' => {
            if let Some(ref registry) = app.web_chat_registry {
                let username = app.profile_state.profile().username.clone();
                let base_url = app
                    .connect_url
                    .rsplit_once('/')
                    .map_or(&*app.connect_url, |p| p.0);
                let token = registry.create_link(app.user_id, username);
                let url = format!("{}/chat/{}", base_url, token);
                app.pending_clipboard = Some(url.clone());
                app.web_chat_qr_url = Some(url);
                app.show_web_chat_qr = true;
            }
            true
        }
        _ => false,
    }
}
