use super::*;

#[test]
fn text_with_caret_uses_cursor_column() {
    assert_eq!(text_with_caret("abcd", 0), "█abcd");
    assert_eq!(text_with_caret("abcd", 2), "ab█cd");
    assert_eq!(text_with_caret("abcd", 4), "abcd█");
    assert_eq!(text_with_caret("abcd", 99), "abcd█");
}

#[test]
fn chat_log_action_span_is_a_static_press_enter_affordance() {
    // Unlike `toggle_span`, this row isn't a toggle - it always renders the
    // same "press enter" hint regardless of any draft/profile state.
    let span = chat_log_action_span();
    assert_eq!(span.text, "↵ view");
}
