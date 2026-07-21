use super::*;

#[test]
fn close_keys_include_printable_q_variants() {
    assert!(is_close_event(&ParsedInput::Char('q')));
    assert!(is_close_event(&ParsedInput::Char('Q')));
    assert!(is_close_event(&ParsedInput::Byte(b'q')));
    assert!(is_close_event(&ParsedInput::Byte(b'Q')));
    assert!(is_close_event(&ParsedInput::Byte(0x1B)));
    assert!(!is_close_event(&ParsedInput::Char('j')));
}
