use super::description_summary_lines;

#[test]
fn description_summary_wraps_to_visual_line_budget() {
    let (lines, truncated) = description_summary_lines("hello wide world\nsecond line", 8, 3);

    assert_eq!(lines, vec!["hello", "wide", "world"]);
    assert!(truncated);
}

#[test]
fn description_summary_preserves_short_multiline_description() {
    let (lines, truncated) = description_summary_lines("one\ntwo\nthree", 20, 3);

    assert_eq!(lines, vec!["one", "two", "three"]);
    assert!(!truncated);
}
