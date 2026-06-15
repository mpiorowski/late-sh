use crate::app::{common::primitives::Banner, input::ParsedInput, state::App};

pub fn handle_input(app: &mut App, event: &ParsedInput) -> bool {
    if !app.is_admin {
        app.banner = Some(Banner::error("Admin access required"));
        return true;
    }

    if app.hub_admin_state.is_editing() {
        match event {
            ParsedInput::Byte(0x1B) => {
                if let Some(banner) = app.hub_admin_state.cancel_edit() {
                    app.banner = Some(banner);
                }
            }
            ParsedInput::Byte(b'\r' | b'\n') => {
                if let Some(banner) = app.hub_admin_state.commit_edit() {
                    app.banner = Some(banner);
                }
            }
            ParsedInput::Byte(0x08 | 0x7F) | ParsedInput::CtrlBackspace => {
                app.hub_admin_state.backspace_edit();
            }
            ParsedInput::Delete => app.hub_admin_state.delete_edit(),
            ParsedInput::CtrlDelete => app.hub_admin_state.clear_edit(),
            ParsedInput::Arrow(b'C') => app.hub_admin_state.move_edit_cursor(1),
            ParsedInput::Arrow(b'D') => app.hub_admin_state.move_edit_cursor(-1),
            ParsedInput::Home => app.hub_admin_state.edit_cursor_home(),
            ParsedInput::End => app.hub_admin_state.edit_cursor_end(),
            ParsedInput::Char(ch) => app.hub_admin_state.push_edit_char(*ch),
            _ => {}
        }
        return true;
    }

    match event {
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.hub_admin_state.move_selection(-1);
            true
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.hub_admin_state.move_selection(1);
            true
        }
        ParsedInput::Arrow(b'C')
        | ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L') => {
            app.hub_admin_state.select_next_field();
            true
        }
        ParsedInput::Arrow(b'D')
        | ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H') => {
            app.hub_admin_state.select_previous_field();
            true
        }
        ParsedInput::Byte(b'[') | ParsedInput::Char('[') => {
            app.hub_admin_state.select_previous_category();
            true
        }
        ParsedInput::Byte(b']') | ParsedInput::Char(']') => {
            app.hub_admin_state.select_next_category();
            true
        }
        ParsedInput::Byte(b'\r' | b'\n') | ParsedInput::Char('e' | 'E') => {
            if let Some(banner) = app.hub_admin_state.begin_edit() {
                app.banner = Some(banner);
            }
            true
        }
        ParsedInput::Byte(b'+' | b'=') | ParsedInput::Char('+' | '=') => {
            if let Some(banner) = app.hub_admin_state.adjust_or_toggle(1) {
                app.banner = Some(banner);
            }
            true
        }
        ParsedInput::Byte(b'-' | b'_') | ParsedInput::Char('-' | '_') => {
            if let Some(banner) = app.hub_admin_state.adjust_or_toggle(-1) {
                app.banner = Some(banner);
            }
            true
        }
        ParsedInput::Byte(b's' | b'S') | ParsedInput::Char('s' | 'S') => {
            if let Some(banner) = app.hub_admin_state.save(app.is_admin) {
                app.banner = Some(banner);
            }
            true
        }
        ParsedInput::Byte(b'r' | b'R') | ParsedInput::Char('r' | 'R') => {
            app.hub_admin_state.reload(app.is_admin);
            app.banner = Some(Banner::success("Reloading reward templates"));
            true
        }
        _ => false,
    }
}
