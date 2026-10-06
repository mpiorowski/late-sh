use super::poll_option_position;

#[test]
fn poll_vote_suffixes_are_letters() {
    assert_eq!(poll_option_position(b'a'), Some(1));
    assert_eq!(poll_option_position(b'b'), Some(2));
    assert_eq!(poll_option_position(b'c'), Some(3));
    assert_eq!(poll_option_position(b'A'), Some(1));
    assert_eq!(poll_option_position(b'B'), Some(2));
    assert_eq!(poll_option_position(b'C'), Some(3));
}

#[test]
fn numeric_suffixes_remain_available_for_music_selection() {
    assert_eq!(poll_option_position(b'1'), None);
    assert_eq!(poll_option_position(b'2'), None);
    assert_eq!(poll_option_position(b'3'), None);
}
