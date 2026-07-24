use super::state::Mode;
use crate::app::input::ParsedInput;
use crate::app::state::App;

/// Route a key/mouse event to the open room-info form.
pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => app.room_info_modal_state.close(),
        ParsedInput::Byte(b'\t') | ParsedInput::Arrow(b'B') => {
            app.room_info_modal_state.focus_next()
        }
        ParsedInput::BackTab | ParsedInput::Arrow(b'A') => app.room_info_modal_state.focus_prev(),
        ParsedInput::Byte(b'\r') => submit(app),
        ParsedInput::Byte(0x15) => app.room_info_modal_state.clear_active(),
        ParsedInput::Byte(0x01) | ParsedInput::Home => app.room_info_modal_state.cursor_home(),
        ParsedInput::Byte(0x05) | ParsedInput::End => app.room_info_modal_state.cursor_end(),
        ParsedInput::Byte(0x7F | 0x08) => app.room_info_modal_state.backspace(),
        ParsedInput::Delete => app.room_info_modal_state.delete_forward(),
        ParsedInput::Arrow(b'C') => app.room_info_modal_state.cursor_right(),
        ParsedInput::Arrow(b'D') => app.room_info_modal_state.cursor_left(),
        ParsedInput::Paste(pasted) => {
            let text = String::from_utf8_lossy(&pasted);
            for ch in text.chars() {
                if !ch.is_control() {
                    app.room_info_modal_state.push(ch);
                }
            }
        }
        ParsedInput::Char(ch) if !ch.is_control() => app.room_info_modal_state.push(ch),
        ParsedInput::Byte(byte) if byte.is_ascii_graphic() || byte == b' ' => {
            app.room_info_modal_state.push(byte as char)
        }
        _ => {}
    }
}

/// Validate and dispatch the form. A name is required; about/rules are optional.
fn submit(app: &mut App) {
    let (title, about, rules) = app.room_info_modal_state.values();
    if title.is_empty() {
        app.room_info_modal_state
            .set_status("Please give your room a name.");
        return;
    }
    let Some(mode) = app.room_info_modal_state.mode().cloned() else {
        app.room_info_modal_state.close();
        return;
    };
    let user_id = app.user_id;
    let opt = |s: String| (!s.is_empty()).then_some(s);
    match mode {
        Mode::Create { is_private, slug } => {
            app.chat.service.create_room_with_info_task(
                user_id,
                is_private,
                slug,
                title,
                opt(about),
                opt(rules),
            );
        }
        Mode::Edit { room_id } => {
            app.chat
                .service
                .set_room_info_task(user_id, room_id, title, opt(about), opt(rules));
        }
    }
    app.room_info_modal_state.close();
}
