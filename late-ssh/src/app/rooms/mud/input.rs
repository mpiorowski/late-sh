// Key routing for Lateania.
//
// IMPORTANT constraint: the rooms layer routes several keys to the embedded
// chat before a game ever sees them - notably `i`, `j`, `k` (and `d`/`r`/`e`/
// `p`/`c`/`f`/`g` while a chat message is selected). See
// `rooms/input.rs::should_route_active_room_chat_key`. So the slice avoids those
// for movement and uses a key scheme that survives the chat-first heuristic:
//
//   arrows / w a s d   movement (n/s via w/s, e/w via d/a)
//   < and >            up / down
//   space or x         attack what's here
//   z                  flee
//   l                  look again
//   Esc / q            leave the world
//
// A full typed MUD prompt ("attack goblin", "look chest") needs an input-capture
// mode that suppresses chat routing; that is deferred to a later phase and noted
// in the design docs.

use crate::app::rooms::{backend::InputAction, mud::state::State, mud::world::Dir};

pub fn handle_key(state: &mut State, byte: u8) -> InputAction {
    match byte {
        0x1B | b'q' | b'Q' => {
            state.leave_world();
            InputAction::Leave
        }
        b'w' | b'W' => {
            state.go(Dir::North);
            InputAction::Handled
        }
        b's' | b'S' => {
            state.go(Dir::South);
            InputAction::Handled
        }
        b'a' | b'A' | b'h' | b'H' => {
            state.go(Dir::West);
            InputAction::Handled
        }
        b'd' | b'D' | b'l' | b'L' => {
            // `l` doubles as east here for convenience; `d` is east. (Both are
            // safe: chat only claims `d`/`l` when a message is selected, in which
            // case the player is interacting with chat anyway.)
            state.go(Dir::East);
            InputAction::Handled
        }
        b'<' | b',' => {
            state.go(Dir::Up);
            InputAction::Handled
        }
        b'>' | b'.' => {
            state.go(Dir::Down);
            InputAction::Handled
        }
        b' ' | b'x' | b'X' | b'\r' | b'\n' => {
            state.attack();
            InputAction::Handled
        }
        b'z' | b'Z' => {
            state.flee();
            InputAction::Handled
        }
        b'o' | b'O' => {
            state.look();
            InputAction::Handled
        }
        _ => InputAction::Ignored,
    }
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    match key {
        b'A' => state.go(Dir::North),
        b'B' => state.go(Dir::South),
        b'C' => state.go(Dir::East),
        b'D' => state.go(Dir::West),
        _ => return false,
    }
    true
}

/// The four CSI arrow finals this game consumes. Exposed for the input-routing
/// test so the mapping stays in one place.
pub fn arrow_maps_to_direction(key: u8) -> Option<Dir> {
    match key {
        b'A' => Some(Dir::North),
        b'B' => Some(Dir::South),
        b'C' => Some(Dir::East),
        b'D' => Some(Dir::West),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_map_to_the_four_compass_directions() {
        assert_eq!(arrow_maps_to_direction(b'A'), Some(Dir::North));
        assert_eq!(arrow_maps_to_direction(b'B'), Some(Dir::South));
        assert_eq!(arrow_maps_to_direction(b'C'), Some(Dir::East));
        assert_eq!(arrow_maps_to_direction(b'D'), Some(Dir::West));
        assert_eq!(arrow_maps_to_direction(b'Z'), None);
    }
}
