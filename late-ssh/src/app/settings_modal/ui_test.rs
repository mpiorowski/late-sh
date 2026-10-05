use super::*;

#[test]
fn text_with_caret_uses_cursor_column() {
    assert_eq!(text_with_caret("abcd", 0), "█abcd");
    assert_eq!(text_with_caret("abcd", 2), "ab█cd");
    assert_eq!(text_with_caret("abcd", 4), "abcd█");
    assert_eq!(text_with_caret("abcd", 99), "abcd█");
}

#[test]
fn labels_shorten_at_grapheme_boundaries_using_rendered_widths() {
    assert_eq!(fit_label("e\u{301}日本", 4), "e\u{301}日 ");
    assert_eq!(Span::raw(fit_label("日本", 2)).width(), 2);
    assert_eq!(fit_label("Label", 0), "");
    assert_eq!(
        statusline_list_width(),
        Span::raw(">[ ] Users online [↑↓]").width() as u16
    );
}
