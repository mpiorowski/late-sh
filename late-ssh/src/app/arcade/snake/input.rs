use super::state::{Direction, State};

fn canonical_direction_key(byte: u8) -> Option<Direction> {
    match byte {
        b'k' | b'K' | b'w' | b'W' => Some(Direction::Up),
        b'j' | b'J' | b's' | b'S' => Some(Direction::Down),
        b'l' | b'L' | b'd' | b'D' => Some(Direction::Right),
        b'h' | b'H' | b'a' | b'A' => Some(Direction::Left),
        _ => None,
    }
}

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    match byte {
        b'r' | b'R' | b'n' | b'N' => {
            state.reset_game();
            true
        }
        b'p' | b'P' => {
            state.toggle_pause();
            true
        }
        byte => {
            let Some(direction) = canonical_direction_key(byte) else {
                return false;
            };
            state.input_queue.insert(0, direction);
            true
        }
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    let direction = match key {
        b'A' => Direction::Up,
        b'B' => Direction::Down,
        b'C' => Direction::Right,
        b'D' => Direction::Left,
        _ => return false,
    };
    state.input_queue.insert(0, direction);
    true
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
