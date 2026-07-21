use super::*;

#[test]
fn control_resize_parses() {
    let parsed: ControlFrame =
        serde_json::from_str(r#"{"t":"resize","cols":120,"rows":40}"#).unwrap();
    assert_eq!(
        parsed,
        ControlFrame::Resize {
            cols: 120,
            rows: 40
        }
    );
}

#[test]
fn constant_time_eq_basic_cases() {
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"abcd"));
}

#[test]
fn readonly_input_allows_only_page_navigation() {
    for input in [b"1".as_slice(), b"5", b"\t", b"\x1b", b"\x1b[Z"] {
        assert!(matches!(
            readonly_input_event(input),
            Some(InputEvent::Bytes(_))
        ));
    }
}

#[test]
fn readonly_input_blocks_mutating_and_mouse_input() {
    for input in [
        b"q".as_slice(),
        b"\r",
        b"hello",
        b"\x1b[A",
        b"\x1b[<0;10;10M",
        b"\x1b[200~paste\x1b[201~",
    ] {
        assert!(readonly_input_event(input).is_none());
    }
}
