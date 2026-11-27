use super::text_layout::*;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

#[test]
fn cell_budgets_preserve_complete_graphemes() {
    for glyph in ["☀️", "🌪️", "🇺🇸", "👍🏽", "👨‍👩‍👧", "界", "e\u{301}"]
    {
        for budget in 0..=6 {
            let text = format!("{glyph}AB");
            assert!(width(&clip(&text, budget)) <= budget);
            assert!(width(&ellipsize(&text, budget)) <= budget);
            assert_eq!(width(&fit(&text, budget)), budget);
        }
    }
    assert_eq!(clip("☀️AB", 2), "☀️");
    assert_eq!(ellipsize("☀️AB", 3), "☀️…");
    assert_eq!(clip("界", 1), "");
    assert_eq!(ellipsize("界", 1), "…");
    assert_eq!(ellipsize("AB", 0), "");
    assert_eq!(width("😀\u{fe0e}"), Span::raw("😀\u{fe0e}").width());
}

#[test]
fn scrolling_clips_wide_glyphs_to_spaces() {
    assert_eq!(window("☀️ABC", 0, 2), "☀️");
    assert_eq!(window("☀️ABC", 1, 2), " A");
    assert_eq!(window("A☀️B", 0, 2), "A ");
    assert_eq!(window("A", 2, 2), "  ");
    assert_eq!(window("☀️", 0, 0), "");
}

#[test]
fn selectors_crossing_style_boundaries_stay_with_the_base() {
    let red = Style::default().fg(Color::Red);
    let spans = vec![Span::styled("☀", red), Span::raw("\u{fe0f}AB")];
    let clipped = clip_spans(spans, 3);
    assert_eq!(clipped[0], Span::styled("☀️", red));
    let line = Line::from(clipped);
    assert_eq!(line.width(), 3);
    let area = Rect::new(0, 0, 3, 1);
    let mut buffer = Buffer::empty(area);
    Paragraph::new(line).render(area, &mut buffer);
    assert_eq!(buffer[(0, 0)].symbol(), "☀️");
    assert_eq!(buffer[(2, 0)].symbol(), "…");
}

#[test]
fn wrapping_preserves_sequences_and_utf8_source_ranges() {
    let text = "☀️AB 👨‍👩‍👧界";
    let ranges = wrap_ranges(text, 3, 3, false);
    assert_eq!(
        ranges
            .iter()
            .map(|range| &text[range.clone()])
            .collect::<String>(),
        text
    );
    assert!(ranges.iter().all(|range| width(&text[range.clone()]) <= 3));
}

#[test]
fn wrapping_moves_a_grapheme_past_a_full_or_narrow_first_row() {
    for first_width in [0, 1] {
        let text = "☀️AB";
        let rows = wrap_ranges(text, first_width, 4, false);
        assert_eq!(rows, vec![0..0, 0..text.len()]);
    }
}
