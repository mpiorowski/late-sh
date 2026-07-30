use super::*;
use std::io::Write;

#[test]
fn shared_buffer_write_and_take() {
    let mut buf = SharedBuffer::default();
    buf.write_all(b"hello").unwrap();
    let taken = buf.take();
    assert_eq!(taken, b"hello");
}

#[test]
fn shared_buffer_take_clears() {
    let mut buf = SharedBuffer::default();
    buf.write_all(b"data").unwrap();
    let _ = buf.take();
    assert!(buf.take().is_empty());
}

#[test]
fn shared_buffer_multiple_writes() {
    let mut buf = SharedBuffer::default();
    buf.write_all(b"hello").unwrap();
    buf.write_all(b" world").unwrap();
    assert_eq!(buf.take(), b"hello world");
}

#[test]
fn shared_buffer_flush_succeeds() {
    let mut buf = SharedBuffer::default();
    assert!(buf.flush().is_ok());
}

#[test]
fn shared_buffer_write_returns_correct_len() {
    let mut buf = SharedBuffer::default();
    let written = buf.write(b"test").unwrap();
    assert_eq!(written, 4);
}

#[test]
fn shared_buffer_default_is_empty() {
    let buf = SharedBuffer::default();
    assert!(buf.take().is_empty());
}

#[test]
fn leave_alt_screen_resets_cursor_shape() {
    let bytes = App::leave_alt_screen();
    assert!(
        bytes
            .windows(CURSOR_SHAPE_STEADY_BLOCK.len())
            .any(|w| w == CURSOR_SHAPE_STEADY_BLOCK),
        "expected steady block cursor reset in shutdown bytes, got: {bytes:?}"
    );
}

#[test]
fn alt_screen_boundaries_recover_terminal_string_state() {
    assert!(App::enter_alt_screen().starts_with(terminal_string_terminator()));
    assert!(App::leave_alt_screen().starts_with(terminal_string_terminator()));
}

#[test]
fn cursor_shape_sequences_match_expected_descusr_codes() {
    assert_eq!(CURSOR_SHAPE_STEADY_BLOCK, b"\x1b[2 q");
    assert_eq!(CURSOR_SHAPE_STEADY_UNDERLINE, b"\x1b[4 q");
}

#[test]
fn voice_toggle_intent_joins_when_not_already_in_voice() {
    let active = uuid::Uuid::from_u128(1);

    assert_eq!(
        voice_toggle_intent(None, Some(active)),
        VoiceToggleIntent::JoinOrSwitch
    );
    assert_eq!(
        voice_toggle_intent(None, None),
        VoiceToggleIntent::JoinOrSwitch
    );
}

#[test]
fn voice_toggle_intent_leaves_current_voice_room() {
    let room = uuid::Uuid::from_u128(1);

    assert_eq!(
        voice_toggle_intent(Some(room), Some(room)),
        VoiceToggleIntent::Leave
    );
    assert_eq!(
        voice_toggle_intent(Some(room), None),
        VoiceToggleIntent::Leave
    );
}

#[test]
fn voice_toggle_intent_switches_to_active_voice_room() {
    let joined = uuid::Uuid::from_u128(1);
    let active = uuid::Uuid::from_u128(2);

    assert_eq!(
        voice_toggle_intent(Some(joined), Some(active)),
        VoiceToggleIntent::JoinOrSwitch
    );
}
