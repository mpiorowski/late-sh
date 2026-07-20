use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use crate::app::common::markdown::*;

fn lines_to_strings(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn renders_inline_bold_italic_code_strike() {
    let lines = render_body_to_lines(
        "**bold** *italic* `code` ***both*** ~~gone~~",
        80,
        Span::raw(""),
        Style::default(),
    );
    let spans = &lines[0].spans;
    assert!(spans.iter().any(|s| {
        s.content.as_ref() == "bold" && s.style.add_modifier.contains(Modifier::BOLD)
    }));
    assert!(spans.iter().any(|s| {
        s.content.as_ref() == "italic" && s.style.add_modifier.contains(Modifier::ITALIC)
    }));
    assert!(spans.iter().any(|s| {
        s.content.as_ref().contains("code") && s.style.bg == Some(theme::BG_HIGHLIGHT())
    }));
    assert!(spans.iter().any(|s| {
        s.content.as_ref() == "both"
            && s.style.add_modifier.contains(Modifier::BOLD)
            && s.style.add_modifier.contains(Modifier::ITALIC)
    }));
    assert!(spans.iter().any(|s| {
        s.content.as_ref() == "gone" && s.style.add_modifier.contains(Modifier::CROSSED_OUT)
    }));
}

#[test]
fn renders_link_with_underline_and_url() {
    let lines = render_body_to_lines(
        "see [docs](https://example.com) here",
        80,
        Span::raw(""),
        Style::default(),
    );
    let link_text = lines[0]
        .spans
        .iter()
        .find(|s| s.content.as_ref() == "docs")
        .expect("link text");
    assert_eq!(link_text.style.fg, Some(theme::AMBER()));
    assert!(link_text.style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn renders_heading_with_glyph() {
    let lines = render_body_to_lines("# title", 80, Span::raw(""), Style::default());
    let glyph = lines[0]
        .spans
        .iter()
        .find(|s| s.content.as_ref() == "▍ ")
        .expect("glyph span");
    assert_eq!(glyph.style.fg, Some(theme::AMBER_GLOW()));
}

#[test]
fn renders_fenced_code_block() {
    let lines = render_body_to_lines(
        "```\nlet x = 1;\n**not bold**\n```",
        80,
        Span::raw(""),
        Style::default(),
    );
    let rendered = lines_to_strings(&lines).join("\n");
    assert!(rendered.contains("let x = 1;"));
    assert!(rendered.contains("**not bold**"));
    for line in &lines {
        assert!(
            line.spans
                .iter()
                .any(|s| s.style.bg == Some(theme::BG_HIGHLIGHT()))
        );
    }
}

#[test]
fn renders_inline_code_without_mention_highlight() {
    let lines =
        render_body_to_lines("look at `@graybeard`", 80, Span::raw(""), Style::default());
    let code_span = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("@graybeard"))
        .expect("code span");
    assert!(!code_span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(code_span.style.bg, Some(theme::BG_HIGHLIGHT()));
}

#[test]
fn renders_inline_code_with_embedded_backtick() {
    let lines = render_body_to_lines("``(╯`Д´)╯︵ ┻━┻``", 80, Span::raw(""), Style::default());
    assert_eq!(lines_to_strings(&lines), vec![" (╯`Д´)╯︵ ┻━┻ "]);
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|span| span.content.contains("(╯`Д´)╯︵ ┻━┻")
                && span.style.bg == Some(theme::BG_HIGHLIGHT()))
    );
}

#[test]
fn renders_ordered_list() {
    let lines = render_body_to_lines(
        "1. first\n2. second\n10. tenth",
        80,
        Span::raw(""),
        Style::default(),
    );
    let strings = lines_to_strings(&lines);
    assert_eq!(strings.len(), 3);
    assert!(strings[0].starts_with("1. first"));
    assert!(strings[1].starts_with("2. second"));
    assert!(strings[2].starts_with("10. tenth"));
}

#[test]
fn ordered_list_continuations_align_under_text() {
    let lines = render_body_to_lines("1. hello wide world", 8, Span::raw(""), Style::default());
    let strings = lines_to_strings(&lines);
    assert!(strings[0].starts_with("1. "));
    for cont in &strings[1..] {
        assert!(cont.starts_with("   "), "continuation {cont:?} misaligned");
    }
}

#[test]
fn wrap_plain_line_preserves_leading_spaces() {
    let result = wrap_plain_line("   hello", 40);
    assert_eq!(result, vec!["   hello"]);
}

#[test]
fn wrap_plain_line_wraps_at_width() {
    let result = wrap_plain_line("hello world", 7);
    assert_eq!(result, vec!["hello ", "world"]);
}

#[test]
fn wrap_plain_line_breaks_long_word() {
    let result = wrap_plain_line("abcdefgh", 4);
    assert_eq!(result, vec!["abcd", "efgh"]);
}

#[test]
fn wrap_plain_line_respects_display_width() {
    let result = wrap_plain_line("(∩｀-´)⊃━☆ﾟ.*･｡ﾟ", 10);
    assert!(
        result
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 10),
        "wrapped rows exceeded display width: {result:?}"
    );
}

#[test]
fn render_body_to_lines_respects_display_width() {
    let lines = render_body_to_lines("(∩｀-´)⊃━☆ﾟ.*･｡ﾟ", 10, Span::raw(""), Style::default());
    for line in lines_to_strings(&lines) {
        assert!(
            UnicodeWidthStr::width(line.as_str()) <= 10,
            "line exceeded display width: {line:?}"
        );
    }
}

#[test]
fn pad_to_width_respects_display_width() {
    let padded = pad_to_width("ab｀", 4);
    assert_eq!(UnicodeWidthStr::width(padded.as_str()), 4);
}

#[test]
fn wrap_plain_line_empty_returns_empty() {
    let result = wrap_plain_line("", 40);
    assert!(result.is_empty());
}
