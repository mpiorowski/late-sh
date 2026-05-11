use super::state::State;

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'n' | b'N' => {
            state.new_personal_board();
            true
        }
        b'[' => {
            state.prev_difficulty();
            true
        }
        b']' => {
            state.next_difficulty();
            true
        }
        b'p' | b'P' => {
            state.show_personal();
            true
        }
        b'd' | b'D' => {
            state.show_daily();
            true
        }
        b'{' => {
            state.scroll_up();
            true
        }
        b'}' => {
            state.scroll_down();
            true
        }
        b'r' | b'R' => {
            state.reset_board();
            true
        }
        b'a' | b'A' => state.auto_move(),
        b'f' | b'F' => state.auto_foundation_all(),
        b'u' | b'U' => state.undo(),
        b'h' | b'H' => {
            state.move_horizontal(-1);
            true
        }
        b'l' | b'L' => {
            state.move_horizontal(1);
            true
        }
        b'k' | b'K' => {
            state.move_vertical(-1);
            true
        }
        b'j' | b'J' => {
            state.move_vertical(1);
            true
        }
        b' ' | b'\r' | b'\n' => state.activate(),
        b'c' | b'C' | 0x1B => {
            state.selection = None;
            true
        }
        _ => false,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => {
            state.move_vertical(-1);
            true
        }
        b'B' => {
            state.move_vertical(1);
            true
        }
        b'C' => {
            state.move_horizontal(1);
            true
        }
        b'D' => {
            state.move_horizontal(-1);
            true
        }
        _ => false,
    }
}
