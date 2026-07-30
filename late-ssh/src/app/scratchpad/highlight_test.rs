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

/// Whether a call re-parsed is observable without reaching inside the cache:
/// a reused render comes back at the length it was originally built to, while
/// a fresh one comes back at exactly the viewport asked for. So a longer
/// result than requested means a hit, and an exact-length one means a miss.
#[test]
fn cache_reuses_the_render_until_the_text_or_language_changes() {
    let mut cache = HighlightCache::default();
    let lines: Vec<String> = (0..8).map(|n| format!("let x{n} = {n};")).collect();

    let first = cache.body(&lines, Language::Rust, 8);
    assert_eq!(first.len(), 8);

    // A cursor move, a scroll, a resize, a partner cursor update: the text is
    // untouched, so syntect must not run again.
    let hit = cache.body(&lines, Language::Rust, 4);
    assert_eq!(hit.len(), 8, "unchanged text must not re-parse");
    assert_eq!(hit, first);

    let after_language = cache.body(&lines, Language::Python, 4);
    assert_eq!(after_language.len(), 4, "a language cycle re-parses");

    assert_eq!(cache.body(&lines, Language::Python, 8).len(), 8);
    let mut edited = lines.clone();
    edited[1] = "let x1 = 99;".to_string();
    let after_edit = cache.body(&edited, Language::Python, 4);
    assert_eq!(after_edit.len(), 4, "a keystroke re-parses");
}

#[test]
fn cache_styles_only_down_to_the_viewport() {
    let mut cache = HighlightCache::default();
    let lines: Vec<String> = (0..100).map(|n| format!("let x{n} = {n};")).collect();

    // A 10-row viewport at the top of a 100-line buffer: the 90 lines below
    // the fold are never styled, which is where the cost lives.
    assert_eq!(cache.body(&lines, Language::Rust, 10).len(), 10);

    // Scrolling further down needs lines the cached render stopped short of.
    assert_eq!(cache.body(&lines, Language::Rust, 40).len(), 40);

    // Scrolling back up is served from the deeper render already in hand.
    assert_eq!(
        cache.body(&lines, Language::Rust, 10).len(),
        40,
        "a shorter viewport is still covered"
    );

    // A viewport past the end of the buffer is clamped, not padded.
    assert_eq!(cache.body(&lines, Language::Rust, 500).len(), 100);
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
