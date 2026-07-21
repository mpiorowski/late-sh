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
