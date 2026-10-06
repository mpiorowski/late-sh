use super::movement_direction_for_key;
use asterion_core::Direction;

#[test]
fn movement_keys_include_wasd_and_legacy_hl() {
    assert_eq!(movement_direction_for_key(b'w'), Some(Direction::North));
    assert_eq!(movement_direction_for_key(b's'), Some(Direction::South));
    assert_eq!(movement_direction_for_key(b'a'), Some(Direction::West));
    assert_eq!(movement_direction_for_key(b'd'), Some(Direction::East));
    assert_eq!(movement_direction_for_key(b'h'), Some(Direction::West));
    assert_eq!(movement_direction_for_key(b'l'), Some(Direction::East));
}
