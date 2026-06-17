use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'\r' | b'\n' => state.submit_guess(),
        0x08 | 0x7F => state.pop_letter(),
        b'a'..=b'z' | b'A'..=b'Z' => state.push_letter(byte as char),
        _ => false,
    }
}

pub fn handle_arrow(_state: &mut State, key: u8) -> bool {
    matches!(key, b'A' | b'B' | b'C' | b'D')
}
