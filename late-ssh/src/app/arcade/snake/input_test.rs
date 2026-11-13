use super::{Direction, canonical_direction_key};

#[test]
fn wasd_and_hjkl_map_to_directions() {
    assert_eq!(canonical_direction_key(b'w'), Some(Direction::Up));
    assert_eq!(canonical_direction_key(b'k'), Some(Direction::Up));
    assert_eq!(canonical_direction_key(b's'), Some(Direction::Down));
    assert_eq!(canonical_direction_key(b'j'), Some(Direction::Down));
    assert_eq!(canonical_direction_key(b'd'), Some(Direction::Right));
    assert_eq!(canonical_direction_key(b'l'), Some(Direction::Right));
    assert_eq!(canonical_direction_key(b'a'), Some(Direction::Left));
    assert_eq!(canonical_direction_key(b'h'), Some(Direction::Left));
}

#[test]
fn uppercase_wasd_do_not_collide_with_arrow_meanings() {
    assert_eq!(canonical_direction_key(b'W'), Some(Direction::Up));
    assert_eq!(canonical_direction_key(b'S'), Some(Direction::Down));
    assert_eq!(canonical_direction_key(b'D'), Some(Direction::Right));
    assert_eq!(canonical_direction_key(b'A'), Some(Direction::Left));
}
