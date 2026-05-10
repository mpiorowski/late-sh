use super::state::State;

fn canonical_direction_key(byte: u8) -> Option<u8> {
    match byte {
        b'k' | b'K' | b'w' | b'W' => Some(b'A'),
        b'j' | b'J' | b's' | b'S' => Some(b'B'),
        b'l' | b'L' | b'd' | b'D' => Some(b'C'),
        b'h' | b'H' | b'a' | b'A' => Some(b'D'),
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
            let Some(direction_key) = canonical_direction_key(byte) else {
                return false;
            };
            state.input_queue.insert(0, direction_key);
            true
        }
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' | b'B' | b'C' | b'D' => {
            state.input_queue.insert(0, key);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_direction_key;

    #[test]
    fn wasd_and_hjkl_map_to_arrow_direction_bytes() {
        assert_eq!(canonical_direction_key(b'w'), Some(b'A'));
        assert_eq!(canonical_direction_key(b'k'), Some(b'A'));
        assert_eq!(canonical_direction_key(b's'), Some(b'B'));
        assert_eq!(canonical_direction_key(b'j'), Some(b'B'));
        assert_eq!(canonical_direction_key(b'd'), Some(b'C'));
        assert_eq!(canonical_direction_key(b'l'), Some(b'C'));
        assert_eq!(canonical_direction_key(b'a'), Some(b'D'));
        assert_eq!(canonical_direction_key(b'h'), Some(b'D'));
    }

    #[test]
    fn uppercase_wasd_do_not_collide_with_arrow_meanings() {
        assert_eq!(canonical_direction_key(b'W'), Some(b'A'));
        assert_eq!(canonical_direction_key(b'S'), Some(b'B'));
        assert_eq!(canonical_direction_key(b'D'), Some(b'C'));
        assert_eq!(canonical_direction_key(b'A'), Some(b'D'));
    }
}
