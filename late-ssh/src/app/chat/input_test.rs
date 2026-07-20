use super::{is_next_room_key, is_prev_room_key, leader_reaction_emoji};

#[test]
fn next_room_keys_include_ctrl_n() {
    assert!(is_next_room_key(b'l'));
    assert!(is_next_room_key(b'L'));
    assert!(is_next_room_key(0x0E));
    assert!(!is_next_room_key(b'h'));
}

#[test]
fn prev_room_keys_include_ctrl_p() {
    assert!(is_prev_room_key(b'h'));
    assert!(is_prev_room_key(b'H'));
    assert!(is_prev_room_key(0x10));
    assert!(!is_prev_room_key(b'l'));
}

#[test]
fn leader_reaction_keys_are_plain_digits_except_custom_zero() {
    assert_eq!(leader_reaction_emoji(b'0'), None);
    assert_eq!(leader_reaction_emoji(b'1'), Some("👍"));
    assert_eq!(leader_reaction_emoji(b'5'), Some("🔥"));
    assert_eq!(leader_reaction_emoji(b'6'), Some("🙌"));
    assert_eq!(leader_reaction_emoji(b'7'), Some("🚀"));
    assert_eq!(leader_reaction_emoji(b'8'), Some("🤔"));
    assert_eq!(leader_reaction_emoji(b'9'), Some("💩"));
    assert_eq!(leader_reaction_emoji(b'!'), None);
}
