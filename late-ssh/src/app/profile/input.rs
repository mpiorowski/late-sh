use crate::app::state::App;

fn settings_row_delta(byte: u8) -> Option<isize> {
    match byte {
        b'j' | b'J' => Some(1),
        b'k' | b'K' => Some(-1),
        _ => None,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) {
    if byte == b'i' {
        app.profile_state.start_username_edit();
        return;
    }
    if let Some(delta) = settings_row_delta(byte) {
        app.profile_state.move_settings_row(delta);
        return;
    }
    match byte {
        b' ' | b'\r' => app.profile_state.cycle_setting(true),
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        // Left/Right = cycle the selected setting value
        b'C' | b'D' => {
            app.profile_state.cycle_setting(key == b'C');
            true
        }
        // Up/Down = move between settings rows
        b'A' => {
            app.profile_state.move_settings_row(-1);
            true
        }
        b'B' => {
            app.profile_state.move_settings_row(1);
            true
        }
        _ => false,
    }
}

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        b'\r' => app.profile_state.submit_username(),
        0x1B => app.profile_state.cancel_username_edit(),
        0x15 => app.profile_state.composer_clear(),
        0x7F => app.profile_state.composer_backspace(),
        _ => {}
    }
}

pub fn handle_composer_char(app: &mut App, ch: char) {
    app.profile_state.composer_push(ch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_row_delta_maps_jk_keys() {
        assert_eq!(settings_row_delta(b'j'), Some(1));
        assert_eq!(settings_row_delta(b'J'), Some(1));
        assert_eq!(settings_row_delta(b'k'), Some(-1));
        assert_eq!(settings_row_delta(b'K'), Some(-1));
        assert_eq!(settings_row_delta(b'x'), None);
    }
}
