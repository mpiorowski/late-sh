use super::{display_link, summary_lines, truncate_to_width};

#[test]
fn summary_lines_wrap_to_budget() {
    let (lines, truncated) = summary_lines("hello wide world", 8, 2);
    assert_eq!(lines, vec!["hello", "wide"]);
    assert!(truncated);
}

#[test]
fn display_link_strips_protocol_and_trailing_slash() {
    assert_eq!(display_link("https://github.com/me/"), "github.com/me");
    assert_eq!(display_link("http://cv.example/"), "cv.example");
    assert_eq!(display_link("ftp://no-strip"), "ftp://no-strip");
}

#[test]
fn truncate_to_width_appends_ellipsis_when_overflowing() {
    assert_eq!(truncate_to_width("hello", 10), "hello");
    assert_eq!(truncate_to_width("hello world", 8), "hello w…");
    assert_eq!(truncate_to_width("hello", 0), "");
    assert_eq!(truncate_to_width("hello", 1), "…");
}
