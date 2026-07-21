use super::*;

#[test]
fn text_with_caret_uses_cursor_column() {
    assert_eq!(text_with_caret("abcd", 0), "█abcd");
    assert_eq!(text_with_caret("abcd", 2), "ab█cd");
    assert_eq!(text_with_caret("abcd", 4), "abcd█");
    assert_eq!(text_with_caret("abcd", 99), "abcd█");
}
