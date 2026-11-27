use crate::app::common::text_layout;

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

/// Render a free-form markdown body into a list of ratatui `Line`s, each
/// prefixed with `pad` and wrapped to `width`.
pub(crate) fn render_body_to_lines(
    body: &str,
    width: usize,
    pad: Span<'static>,
    body_style: Style,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut code_block: Option<CodeBlock<'_>> = None;

    for paragraph in body.split('\n') {
        if let Some(block) = code_block.as_mut() {
            match paragraph.trim_end().strip_suffix(FENCE) {
                Some(before) => {
                    if !before.trim().is_empty() {
                        block.rows.push(before);
                    }
                    lines.extend(render_code_block(&block.shown_rows(), width, &pad));
                    code_block = None;
                }
                None => block.rows.push(paragraph),
            }
            continue;
        }

        if let Some(after) = paragraph.trim_start().strip_prefix(FENCE) {
            match after.trim_end().strip_suffix(FENCE) {
                // Opened and closed on one line: the text between is the code.
                Some(code) => lines.extend(render_code_block(&[code], width, &pad)),
                None => {
                    code_block = Some(CodeBlock {
                        opener: after,
                        rows: Vec::new(),
                    });
                }
            }
            continue;
        }

        if paragraph.is_empty() {
            lines.push(Line::from(pad.clone()));
            continue;
        }

        let block = parse_block(paragraph);
        lines.extend(render_block(block, width, &pad, body_style));
    }

    // A fence nobody closed still renders as a block.
    if let Some(block) = code_block {
        lines.extend(render_code_block(&block.shown_rows(), width, &pad));
    }

    lines
}

const FENCE: &str = "```";

/// A fenced block being collected. `opener` is whatever followed the
/// opening fence on its own line.
struct CodeBlock<'a> {
    opener: &'a str,
    rows: Vec<&'a str>,
}

impl<'a> CodeBlock<'a> {
    /// The rows to draw. The opener is a language tag, and hidden, only
    /// when it is one word sitting over real code; anything else typed on
    /// the fence line is code, so a message like "```oops" never renders
    /// as an empty box.
    fn shown_rows(&self) -> Vec<&'a str> {
        let opener = self.opener.trim();
        let is_language_tag = !opener.contains(char::is_whitespace) && !self.rows.is_empty();
        match opener.is_empty() || is_language_tag {
            true => self.rows.clone(),
            false => std::iter::once(opener)
                .chain(self.rows.iter().copied())
                .collect(),
        }
    }
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
                vec![pad.clone(), Span::raw(" ".repeat(str_width(glyph)))],
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
            let indent = " ".repeat(str_width(&marker_text));
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

        if let Some((marker_len, end_rel)) = inline_code_bounds(rest) {
            push_plain(&mut spans, &text[plain_start..idx], base_style);
            let inner_start = idx + marker_len;
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
            idx = inner_end + marker_len;
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

fn inline_code_bounds(text: &str) -> Option<(usize, usize)> {
    let marker_len = text.chars().take_while(|ch| *ch == '`').count();
    if marker_len == 0 {
        return None;
    }
    let marker = &text[..marker_len];
    let after_open = &text[marker_len..];
    let end_rel = after_open.find(marker)?;
    (end_rel > 0).then_some((marker_len, end_rel))
}

fn push_plain(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    // The one place plain prose reaches: inline code and fenced blocks are
    // rendered elsewhere, so `:shortcode:` inside code stays literal, which is
    // the whole reason the expansion lives down here rather than on the body.
    let text = super::emoji::expand_shortcodes(text);
    spans.extend(mention_spans(&text, style));
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
    spans.iter().map(Span::width).sum()
}

fn str_width(text: &str) -> usize {
    text_layout::width(text)
}

/// Soft-wrap styled spans into rows, breaking at spaces when possible:
/// the first row `first_width` wide, the rest `continuation_width`.
pub(crate) fn wrap_spans(
    spans: &[Span<'static>],
    first_width: usize,
    continuation_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let symbols = super::text_layout::styled_graphemes(spans);
    let text: String = symbols.iter().map(|g| g.symbol.as_ref()).collect();
    let ranges = super::text_layout::wrap_ranges(&text, first_width, continuation_width, true);
    let mut index = 0;
    let mut offset = 0;
    ranges
        .into_iter()
        .map(|range| {
            let mut row = Vec::new();
            while index < symbols.len() && offset < range.end {
                let g = &symbols[index];
                if offset >= range.start {
                    super::text_layout::push_span(&mut row, &g.symbol, g.style);
                }
                offset += g.symbol.len();
                index += 1;
            }
            row
        })
        .collect()
}

/// Soft-wrap plain text to `width`, breaking at spaces when possible.
pub(crate) fn wrap_plain_line(text: &str, width: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![String::new()];
    }
    super::text_layout::wrap_ranges(text, width, width, false)
        .into_iter()
        .map(|range| text[range].to_string())
        .collect()
}

/// Pad `text` with spaces on the right to exactly `width` display cells,
/// truncating if too wide.
pub(crate) fn pad_to_width(text: &str, width: usize) -> String {
    super::text_layout::fit(text, width)
}
