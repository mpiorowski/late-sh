use crate::app::input::ParsedInput;
use crate::app::state::App;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.show_mod_modal = false,
        ParsedInput::Byte(0x0C) => app.mod_modal_state.clear_log(),
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => submit(app),
        ParsedInput::Byte(0x7F) => app.mod_modal_state.backspace(),
        ParsedInput::Byte(0x08) | ParsedInput::CtrlBackspace => {
            app.mod_modal_state.delete_word_left()
        }
        ParsedInput::Delete => app.mod_modal_state.delete_right(),
        ParsedInput::Arrow(b'C') => app.mod_modal_state.move_right(),
        ParsedInput::Arrow(b'D') => app.mod_modal_state.move_left(),
        ParsedInput::CtrlArrow(b'C') | ParsedInput::AltArrow(b'C') => {
            app.mod_modal_state.move_word_right();
        }
        ParsedInput::CtrlArrow(b'D') | ParsedInput::AltArrow(b'D') => {
            app.mod_modal_state.move_word_left();
        }
        ParsedInput::PageUp => app.mod_modal_state.scroll_log(8),
        ParsedInput::PageDown => app.mod_modal_state.scroll_log(-8),
        ParsedInput::Mouse(mouse) => {
            if let Some(delta) = super::ui::mouse_scroll_delta(mouse) {
                app.mod_modal_state.scroll_log(delta);
            }
        }
        ParsedInput::Char(ch) => app.mod_modal_state.push_char(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.mod_modal_state.push_char(byte as char);
        }
        _ => {}
    }
}

fn submit(app: &mut App) {
    if !app.permissions.can_access_mod_surface() {
        app.mod_modal_state
            .append_error("access denied: moderator or admin only");
        app.mod_modal_state.clear_command();
        return;
    }
    let command = app.mod_modal_state.command_text();
    if command.is_empty() {
        app.mod_modal_state.append_info("type help for commands");
        return;
    }
    app.mod_modal_state.append_input(&command);
    let request_id = app.chat.submit_mod_command(command);
    app.mod_modal_state.append_pending(request_id);
    app.mod_modal_state.clear_command();
}
