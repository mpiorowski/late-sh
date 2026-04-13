use crate::app::state::App;

pub fn handle_byte(app: &mut App, byte: u8) {
    match byte {
        b' ' | b'\r' | b'i' => app.profile_state.start_username_edit(),
        _ => {}
    }
}

pub fn handle_arrow(_app: &mut App, _key: u8) -> bool {
    false
}

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        b'\r' => app.profile_state.submit_username(),
        0x1B => app.profile_state.cancel_username_edit(),
        0x15 => app.profile_state.composer_clear(),
        0x7F => app.profile_state.composer_backspace(),
        b => {
            if let Some(ch) = composer_char_from_byte(b) {
                app.profile_state.composer_push(ch);
            }
        }
    }
}

fn composer_char_from_byte(byte: u8) -> Option<char> {
    if byte.is_ascii_graphic() || byte == b' ' {
        Some(byte as char)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_char_from_byte_accepts_graphics_and_space() {
        assert_eq!(composer_char_from_byte(b'a'), Some('a'));
        assert_eq!(composer_char_from_byte(b'9'), Some('9'));
        assert_eq!(composer_char_from_byte(b' '), Some(' '));
    }

    #[test]
    fn composer_char_from_byte_rejects_control_bytes() {
        assert_eq!(composer_char_from_byte(0x00), None);
        assert_eq!(composer_char_from_byte(0x1B), None);
        assert_eq!(composer_char_from_byte(b'\n'), None);
    }
}
