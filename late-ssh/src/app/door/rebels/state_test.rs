use crate::app::door::rebels::state::*;

#[test]
fn passthrough_non_mouse_bytes() {
    assert_eq!(rewrite_mouse(b"hello\r", 1, 3), b"hello\r");
}

#[test]
fn mouse_position_is_offset_by_viewport() {
    // click at col 5,row 10 with viewport x=1,y=3 -> rebels col 4,row 7
    let input = b"\x1b[<0;5;10M";
    assert_eq!(rewrite_mouse(input, 1, 3), b"\x1b[<0;4;7M".to_vec());
}

#[test]
fn mouse_on_top_bar_is_dropped() {
    // click at row 2 (<= y_offset 3) -> dropped
    let input = b"\x1b[<0;5;2M";
    assert_eq!(rewrite_mouse(input, 1, 3), Vec::<u8>::new());
}

#[test]
fn mouse_on_left_border_is_dropped() {
    // click at col 1 (<= x_offset 1) -> dropped
    let input = b"\x1b[<0;1;10M";
    assert_eq!(rewrite_mouse(input, 1, 3), Vec::<u8>::new());
}

#[test]
fn mixed_stream_keeps_keys_and_rewrites_mouse() {
    let input = b"a\x1b[<0;5;10Mb";
    assert_eq!(rewrite_mouse(input, 1, 3), b"a\x1b[<0;4;7Mb".to_vec());
}

#[test]
fn truncated_mouse_sequence_passes_through_verbatim() {
    // No terminating 'M'/'m' before end-of-buffer: copy bytes through
    // unchanged rather than panicking or swallowing them.
    let input = b"\x1b[<0;5;10";
    assert_eq!(rewrite_mouse(input, 1, 3), input.to_vec());
}

#[test]
fn arrow_key_csi_passes_through_untouched() {
    // `ESC [ A` starts `ESC [` but is not the `ESC [ <` mouse prefix, so it
    // must be forwarded unchanged.
    let input = b"\x1b[A";
    assert_eq!(rewrite_mouse(input, 1, 3), input.to_vec());
}

#[test]
fn connect_is_a_no_op_when_disabled() {
    let mut state = State::new(
        uuid::Uuid::nil(),
        "frittura.org".to_string(),
        3788,
        String::new(),
        "xterm".to_string(),
        false,
        None,
    );
    assert!(!state.is_enabled());
    state.connect();
    // No proxy spawned and we stay in the Launcher.
    assert!(state.proxy().is_none());
    assert_eq!(state.mode(), Mode::Launcher);
}
