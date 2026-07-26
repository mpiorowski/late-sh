use super::*;

#[test]
fn cycle_wraps_forward() {
    assert_eq!(Language::Plain.next(), Language::Rust);
    assert_eq!(Language::Yaml.next(), Language::Plain, "wraps at the end");
}

#[test]
fn plain_never_touches_syntect() {
    let lines = vec!["hello".to_string(), "world".to_string()];
    let rendered = highlighted_lines(&lines, Language::Plain);

    assert_eq!(rendered.len(), 2);
    // Exactly one content span per line, no gutter mixed in: no per-token
    // spans, proving the syntect path was skipped entirely.
    assert_eq!(rendered[0].spans.len(), 1);
    assert_eq!(rendered[1].spans.len(), 1);
}

#[test]
fn rust_snippet_produces_more_than_one_style() {
    let lines = vec!["fn main() {".to_string(), "    let x = 1;".to_string()];
    let rendered = highlighted_lines(&lines, Language::Rust);

    assert_eq!(rendered.len(), 2);
    let distinct_styles: std::collections::HashSet<_> = rendered
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.style))
        .collect();
    assert!(
        distinct_styles.len() > 1,
        "expected keyword/plain-text spans to differ, got {distinct_styles:?}"
    );
}

#[test]
fn gutter_width_matches_total_line_count_digits() {
    let rendered = gutter_lines(150);
    let first_span_text = rendered[0].spans[0].content.as_ref();
    assert_eq!(first_span_text, "  1 ", "3-digit gutter for 150 lines");
    assert_eq!(rendered.len(), 150);
}

#[test]
fn gutter_stays_separate_from_content_so_it_cannot_scroll_away_with_it() {
    // Regression test: the gutter used to be a prefix span baked into the
    // same Line as the content, so ratatui's Paragraph::scroll shifted it
    // sideways along with a long line's horizontal scroll. gutter_lines and
    // highlighted_lines must be independently renderable Paragraphs.
    let lines = vec!["a".to_string()];
    let content = highlighted_lines(&lines, Language::Plain);
    let gutter = gutter_lines(lines.len());

    assert_eq!(
        content[0].spans.len(),
        1,
        "no gutter span mixed into content"
    );
    assert_eq!(content[0].spans[0].content.as_ref(), "a");
    assert_eq!(gutter[0].spans[0].content.as_ref(), "1 ");
}
