use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::{mentions::mention_spans, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownBlock<'a> {
    Paragraph(&'a str),
    Heading { level: u8, text: &'a str },
    Quote(&'a str),
    ListItem(&'a str),
    OrderedListItem { marker: &'a str, text: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StyledChar {
    ch: char,
    style: Style,
}

/// Render a free-form markdown body into a list of ratatui `Line`s, each
/// prefixed with `pad` and wrapped to `width`.
pub(crate) fn render_body_to_lines(
    body: &str,
    width: usize,
    pad: Span<'static>,
    body_style: Style,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut code_buffer: Option<Vec<&str>> = None;

    for paragraph in body.split('\n') {
        if let Some(buf) = code_buffer.as_mut() {
            if paragraph.trim_start().starts_with("```") {
                lines.extend(render_code_block(buf, width, &pad));
                code_buffer = None;
            } else {
                buf.push(paragraph);
            }
            continue;
        }

        if paragraph.trim_start().starts_with("```") {
            code_buffer = Some(Vec::new());
            continue;
        }

        if paragraph.is_empty() {
            lines.push(Line::from(pad.clone()));
            continue;
        }

        let block = parse_block(paragraph);
        lines.extend(render_block(block, width, &pad, body_style));
    }

    if let Some(buf) = code_buffer {
        lines.extend(render_code_block(&buf, width, &pad));
    }

    lines
}

fn render_code_block(rows: &[&str], width: usize, pad: &Span<'static>) -> Vec<Line<'static>> {
    let code_style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT());
    let inner_width = width.saturating_sub(1).max(1);
    let left_pad_width = 2usize;
    let text_width = inner_width.saturating_sub(left_pad_width).max(1);
    let blank_row = " ".repeat(inner_width);
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        pad.clone(),
        Span::styled(blank_row.clone(), code_style),
    ]));

    for row in rows {
        let wrapped = if row.is_empty() {
            vec![String::new()]
        } else {
            let w = wrap_plain_line(row, text_width);
            if w.is_empty() { vec![String::new()] } else { w }
        };
        for chunk in wrapped {
            let mut padded = " ".repeat(left_pad_width);
            padded.push_str(&pad_to_width(&chunk, text_width));
            lines.push(Line::from(vec![
                pad.clone(),
                Span::styled(padded, code_style),
            ]));
        }
    }

    lines.push(Line::from(vec![
        pad.clone(),
        Span::styled(blank_row, code_style),
    ]));

    lines
}

fn parse_block(line: &str) -> MarkdownBlock<'_> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return MarkdownBlock::Paragraph(line);
    }

    if let Some(text) = line.strip_prefix("> ")
        && !text.trim().is_empty()
    {
        return MarkdownBlock::Quote(text);
    }

    if let Some(text) = line.strip_prefix("- ")
        && !text.trim().is_empty()
    {
        return MarkdownBlock::ListItem(text);
    }

    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && let Some(after_dot) = line[digits..].strip_prefix(". ")
        && !after_dot.trim().is_empty()
    {
        return MarkdownBlock::OrderedListItem {
            marker: &line[..digits + 1],
            text: after_dot,
        };
    }

    let heading_level = line.chars().take_while(|ch| *ch == '#').count();
    if (1..=3).contains(&heading_level) {
        let rest = &line[heading_level..];
        if let Some(text) = rest.strip_prefix(' ')
            && !text.trim().is_empty()
        {
            return MarkdownBlock::Heading {
                level: heading_level as u8,
                text,
            };
        }
    }

    MarkdownBlock::Paragraph(line)
}

fn render_block(
    block: MarkdownBlock<'_>,
    width: usize,
    pad: &Span<'static>,
    body_style: Style,
) -> Vec<Line<'static>> {
    match block {
        MarkdownBlock::Paragraph(text) => {
            let content = inline_spans(text, body_style);
            render_wrapped(content, width, vec![pad.clone()], vec![pad.clone()])
        }
        MarkdownBlock::Heading { level, text } => {
            let style = heading_style(level, body_style);
            let glyph = heading_glyph(level);
            let content = inline_spans(text, style);
            let marker = Span::styled(glyph, style);
            render_wrapped(
                content,
                width,
                vec![pad.clone(), marker.clone()],
                vec![pad.clone(), Span::raw(" ".repeat(glyph.chars().count()))],
            )
        }
        MarkdownBlock::Quote(text) => {
            let quote_style = Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC);
            let marker = Span::styled("> ", Style::default().fg(theme::AMBER_DIM()));
            let content = vec![Span::styled(text.to_string(), quote_style)];
            render_wrapped(
                content,
                width,
                vec![pad.clone(), marker.clone()],
                vec![pad.clone(), marker],
            )
        }
        MarkdownBlock::ListItem(text) => {
            let bullet_style = Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD);
            let content = inline_spans(text, body_style);
            render_wrapped(
                content,
                width,
                vec![pad.clone(), Span::styled("• ", bullet_style)],
                vec![pad.clone(), Span::raw("  ")],
            )
        }
        MarkdownBlock::OrderedListItem { marker, text } => {
            let marker_style = Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD);
            let marker_text = format!("{marker} ");
            let indent = " ".repeat(marker_text.chars().count());
            let content = inline_spans(text, body_style);
            render_wrapped(
                content,
                width,
                vec![pad.clone(), Span::styled(marker_text, marker_style)],
                vec![pad.clone(), Span::raw(indent)],
            )
        }
    }
}

fn heading_style(level: u8, base: Style) -> Style {
    match level {
        1 => base.fg(theme::AMBER_GLOW()).add_modifier(Modifier::BOLD),
        2 => base.fg(theme::AMBER()).add_modifier(Modifier::BOLD),
        3 => base.fg(theme::AMBER_DIM()).add_modifier(Modifier::BOLD),
        _ => base,
    }
}

fn heading_glyph(level: u8) -> &'static str {
    match level {
        1 => "▍ ",
        2 => "▎ ",
        3 => "▏ ",
        _ => "",
    }
}

fn inline_spans(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut idx = 0;
    let mut plain_start = 0;

    while idx < text.len() {
        let rest = &text[idx..];

        if let Some(after_open) = rest.strip_prefix("***")
            && let Some(end_rel) = after_open.find("***")
            && end_rel > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + 3;
            let inner_end = inner_start + end_rel;
            push_plain(
                &mut spans,
                &text[inner_start..inner_end],
                base_style.add_modifier(Modifier::BOLD | Modifier::ITALIC),
            );
            idx = inner_end + 3;
            plain_start = idx;
            continue;
        }

        if let Some(after_open) = rest.strip_prefix("**")
            && let Some(end_rel) = after_open.find("**")
            && end_rel > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + 2;
            let inner_end = inner_start + end_rel;
            push_plain(
                &mut spans,
                &text[inner_start..inner_end],
                base_style.add_modifier(Modifier::BOLD),
            );
            idx = inner_end + 2;
            plain_start = idx;
            continue;
        }

        if let Some(after_open) = rest.strip_prefix('`')
            && let Some(end_rel) = after_open.find('`')
            && end_rel > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + 1;
            let inner_end = inner_start + end_rel;
            let code_style = base_style
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_HIGHLIGHT());
            spans.push(Span::styled(" ", code_style));
            spans.push(Span::styled(
                text[inner_start..inner_end].to_string(),
                code_style,
            ));
            spans.push(Span::styled(" ", code_style));
            idx = inner_end + 1;
            plain_start = idx;
            continue;
        }

        if rest.starts_with('[')
            && let Some(bracket_pos) = rest[1..].find(']')
            && bracket_pos > 0
            && let Some(paren_inner) = rest[1 + bracket_pos + 1..].strip_prefix('(')
            && let Some(close_paren) = paren_inner.find(')')
            && close_paren > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let text_start = idx + 1;
            let text_end = text_start + bracket_pos;
            let url_start = text_end + 2;
            let url_end = url_start + close_paren;

            let link_style = base_style
                .fg(theme::AMBER())
                .add_modifier(Modifier::UNDERLINED);
            push_plain(&mut spans, &text[text_start..text_end], link_style);
            spans.push(Span::styled(
                format!(" ({})", &text[url_start..url_end]),
                base_style.fg(theme::TEXT_FAINT()),
            ));

            idx = url_end + 1;
            plain_start = idx;
            continue;
        }

        if let Some(after_open) = rest.strip_prefix('*')
            && !rest.starts_with("**")
            && let Some(end_rel) = after_open.find('*')
            && end_rel > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + 1;
            let inner_end = inner_start + end_rel;
            push_plain(
                &mut spans,
                &text[inner_start..inner_end],
                base_style.add_modifier(Modifier::ITALIC),
            );
            idx = inner_end + 1;
            plain_start = idx;
            continue;
        }

        if let Some(after_open) = rest.strip_prefix("~~")
            && let Some(end_rel) = after_open.find("~~")
            && end_rel > 0
        {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + 2;
            let inner_end = inner_start + end_rel;
            push_plain(
                &mut spans,
                &text[inner_start..inner_end],
                base_style.add_modifier(Modifier::CROSSED_OUT),
            );
            idx = inner_end + 2;
            plain_start = idx;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        idx += ch.len_utf8();
    }

    push_plain(&mut spans, &text[plain_start..], base_style);
    spans
}

fn push_plain(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    spans.extend(mention_spans(text, style));
}

fn render_wrapped(
    content: Vec<Span<'static>>,
    width: usize,
    first_prefix: Vec<Span<'static>>,
    continuation_prefix: Vec<Span<'static>>,
) -> Vec<Line<'static>> {
    if !spans_have_visible_text(&content) {
        return vec![Line::from(first_prefix)];
    }

    let first_width = width.saturating_sub(spans_width(&first_prefix)).max(1);
    let continuation_width = width
        .saturating_sub(spans_width(&continuation_prefix))
        .max(1);
    let rows = wrap_spans(&content, first_width, continuation_width);

    rows.into_iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut spans = if idx == 0 {
                first_prefix.clone()
            } else {
                continuation_prefix.clone()
            };
            spans.extend(row);
            Line::from(spans)
        })
        .collect()
}

fn spans_have_visible_text(spans: &[Span<'static>]) -> bool {
    spans
        .iter()
        .any(|span| span.content.chars().any(|ch| !ch.is_whitespace()))
}

fn spans_width(spans: &[Span<'static>]) -> usize {
    spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum::<usize>()
}

fn wrap_spans(
    spans: &[Span<'static>],
    first_width: usize,
    continuation_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let chars = flatten_spans(spans);
    if chars.is_empty() {
        return Vec::new();
    }

    let mut rows = Vec::new();
    let mut idx = 0;
    while idx < chars.len() {
        let row_width = if rows.is_empty() {
            first_width
        } else {
            continuation_width
        }
        .max(1);
        let end = (idx + row_width).min(chars.len());
        let break_at = if end < chars.len() {
            let mut pos = end;
            while pos > idx && chars[pos - 1].ch != ' ' {
                pos -= 1;
            }
            if pos > idx { pos } else { end }
        } else {
            end
        };

        rows.push(rebuild_spans(&chars[idx..break_at]));
        idx = break_at;
        while idx < chars.len() && chars[idx].ch == ' ' {
            idx += 1;
        }
    }

    rows
}

fn flatten_spans(spans: &[Span<'static>]) -> Vec<StyledChar> {
    let mut chars = Vec::new();
    for span in spans {
        for ch in span.content.chars() {
            chars.push(StyledChar {
                ch,
                style: span.style,
            });
        }
    }
    chars
}

fn rebuild_spans(chars: &[StyledChar]) -> Vec<Span<'static>> {
    let Some(first) = chars.first() else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut current_style = first.style;
    let mut current_text = String::new();

    for styled in chars {
        if styled.style != current_style && !current_text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut current_text),
                current_style,
            ));
            current_style = styled.style;
        }
        current_text.push(styled.ch);
    }

    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }

    spans
}

/// Soft-wrap plain text to `width`, breaking at spaces when possible.
pub(crate) fn wrap_plain_line(text: &str, width: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![String::new()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < chars.len() {
        let end = (idx + width).min(chars.len());
        let break_at = if end < chars.len() {
            let mut pos = end;
            while pos > idx && chars[pos - 1] != ' ' {
                pos -= 1;
            }
            if pos > idx { pos } else { end }
        } else {
            end
        };
        let chunk: String = chars[idx..break_at].iter().collect();
        out.push(chunk);
        idx = break_at;
    }

    out
}

/// Pad `text` with spaces on the right to exactly `width` characters, truncating if too long.
pub(crate) fn pad_to_width(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.chars().take(width).collect();
    }
    let mut out = String::with_capacity(width);
    out.push_str(text);
    out.push_str(&" ".repeat(width - len));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn wrap_plain_line_empty_returns_empty() {
        let result = wrap_plain_line("", 40);
        assert!(result.is_empty());
    }
}
