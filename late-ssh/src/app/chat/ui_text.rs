use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;
use late_core::models::article::NEWS_MARKER;

use super::mentions::{is_mention_char, valid_mention_start};

const NEWS_SEPARATOR: &str = " || ";

// ── Composer text processing ────────────────────────────────

/// Build composer lines with cursor at the given char position.
pub(super) fn build_composer_lines(
    text: &str,
    cursor_pos: usize,
    composing: bool,
    cursor_visible: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let rows = build_composer_rows(text, width);
    build_composer_lines_from_rows(text, &rows, cursor_pos, composing, cursor_visible)
}

pub(super) fn build_composer_lines_from_rows(
    text: &str,
    rows: &[ComposerRow],
    cursor_pos: usize,
    composing: bool,
    cursor_visible: bool,
) -> Vec<Line<'static>> {
    if text.is_empty() {
        if composing {
            let dim = Style::default().fg(theme::TEXT_DIM());
            let first_style = if cursor_visible {
                dim.add_modifier(Modifier::REVERSED)
            } else {
                dim
            };
            return vec![Line::from(vec![
                Span::raw(" "),
                Span::styled("T", first_style),
                Span::styled("ype a message...", dim),
            ])];
        }
        return vec![Line::from("")];
    }
    let cursor_row = if cursor_visible && composing {
        Some(
            rows.iter()
                .position(|row| cursor_pos <= row.end)
                .unwrap_or(rows.len().saturating_sub(1)),
        )
    } else {
        None
    };

    let mut result: Vec<Line> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if let Some(cursor_row_idx) = cursor_row
            && cursor_row_idx == i
        {
            let cursor_col = cursor_pos
                .saturating_sub(row.start)
                .min(row.text.chars().count());
            let row_chars: Vec<char> = row.text.chars().collect();
            let before: String = row_chars.iter().take(cursor_col).collect();
            let at_cursor = row_chars.get(cursor_col).copied();
            let after: String = row_chars.iter().skip(cursor_col + 1).collect();
            let mut spans = vec![Span::raw(" "), Span::raw(before)];
            if let Some(ch) = at_cursor {
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
                if !after.is_empty() {
                    spans.push(Span::raw(after));
                }
            } else {
                // Cursor at end of line
                spans.push(Span::styled(
                    " ",
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
            }
            result.push(Line::from(spans));
        } else {
            let is_last = i == rows.len() - 1;
            let mut spans = vec![Span::raw(" "), Span::raw(row.text.clone())];
            if is_last && !composing {
                // no trailing cursor when not composing
            } else if is_last && composing && cursor_row.is_none() {
                // Keep a trailing cell for the composing cursor state.
                spans.push(Span::raw(" "));
            }
            result.push(Line::from(spans));
        }
    }

    result
}

#[derive(Clone)]
pub struct ComposerRow {
    pub(super) text: String,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) fn build_composer_rows(text: &str, width: usize) -> Vec<ComposerRow> {
    let mut rows = Vec::new();
    let mut offset = 0;

    for paragraph in text.split('\n') {
        let wrapped = wrap_composer_paragraph(paragraph, width);
        if wrapped.is_empty() {
            rows.push(ComposerRow {
                text: String::new(),
                start: offset,
                end: offset,
            });
        } else {
            for (row_text, start, end) in wrapped {
                rows.push(ComposerRow {
                    text: row_text,
                    start: offset + start,
                    end: offset + end,
                });
            }
        }
        offset += paragraph.chars().count() + 1;
    }

    rows
}

fn wrap_composer_paragraph(paragraph: &str, width: usize) -> Vec<(String, usize, usize)> {
    if paragraph.is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![(String::new(), 0, 0)];
    }

    let chars: Vec<char> = paragraph.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + width).min(chars.len());
        if end == chars.len() {
            out.push((chars[start..end].iter().collect(), start, end));
            break;
        }

        let break_at = chars[start..end]
            .iter()
            .rposition(|ch| ch.is_whitespace())
            .map(|idx| start + idx);

        match break_at {
            Some(split) if split > start => {
                out.push((chars[start..split].iter().collect(), start, split));
                start = split + 1;
            }
            _ => {
                out.push((chars[start..end].iter().collect(), start, end));
                start = end;
            }
        }
    }

    out
}

pub(super) fn composer_line_count(text: &str, width: usize) -> usize {
    if text.is_empty() {
        1
    } else {
        build_composer_rows(text, width).len().max(1)
    }
}

pub(crate) fn composer_line_count_for_rows(text: &str, rows: &[ComposerRow]) -> usize {
    if text.is_empty() {
        1
    } else {
        rows.len().max(1)
    }
}

pub(crate) fn composer_cursor_scroll_for_rows(
    rows: &[ComposerRow],
    cursor_pos: usize,
    max_visible: usize,
) -> u16 {
    let total = rows.len().max(1);
    if total <= max_visible {
        return 0;
    }
    let cursor_row = rows
        .iter()
        .position(|r| cursor_pos <= r.end)
        .unwrap_or(total.saturating_sub(1));
    // Keep cursor_row inside the visible window.
    let max_scroll = total.saturating_sub(max_visible);
    // If cursor is below window, scroll down; if above, scroll up.
    // Default: pin to bottom (matches old behaviour when cursor is at end).
    let scroll = cursor_row.saturating_sub(max_visible - 1).min(max_scroll);
    scroll as u16
}

// ── Message wrapping ────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn wrap_message_to_lines(
    body: &str,
    stamp: &str,
    prefix: &str,
    width: usize,
    author_style: Style,
    body_style: Style,
    mentions_us: bool,
    continuation: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let pad = if mentions_us {
        Span::styled("│", Style::default().fg(theme::MENTION()))
    } else {
        Span::raw(" ")
    };

    if !continuation {
        // Line 1: author [time]
        lines.push(Line::from(vec![
            pad.clone(),
            Span::styled(prefix.to_string(), author_style),
            Span::styled(
                format!(" {stamp}"),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    }

    // Line 2+: body with word wrap, respecting newlines
    if body.is_empty() {
        return lines;
    }

    // Each rendered line is prefixed with a 1-column pad span, so the
    // wrappable body width is the area width minus that pad.
    let body_width = width.saturating_sub(1).max(1);

    let (reply_quote, body) = parse_reply_quote(body);
    if let Some(reply_quote) = reply_quote {
        for row in wrap_plain_line(&format!("> {reply_quote}"), body_width) {
            lines.push(Line::from(vec![
                pad.clone(),
                Span::styled(row, Style::default().fg(theme::TEXT_FAINT())),
            ]));
        }
    }

    for paragraph in body.split('\n') {
        if paragraph.is_empty() {
            lines.push(Line::from(pad.clone()));
            continue;
        }
        for chunk in wrap_plain_line(paragraph, body_width) {
            let mut spans = vec![pad.clone()];
            spans.extend(mention_spans(&chunk, body_style));
            lines.push(Line::from(spans));
        }
    }

    lines
}

fn parse_reply_quote(body: &str) -> (Option<String>, &str) {
    let Some((first_line, rest)) = body.split_once('\n') else {
        return (None, body);
    };
    let quote = first_line.trim().strip_prefix("> ").map(str::trim);
    let rest = rest.trim_start_matches('\n');
    match quote {
        Some(quote) if !quote.is_empty() && !rest.trim().is_empty() => {
            (Some(quote.to_string()), rest)
        }
        _ => (None, body),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wrap_chat_entry_to_lines(
    body: &str,
    stamp: &str,
    prefix: &str,
    width: usize,
    author_style: Style,
    body_style: Style,
    mentions_us: bool,
    continuation: bool,
) -> Vec<Line<'static>> {
    if let Some(news) = parse_news_payload(body) {
        return wrap_news_to_lines(stamp, prefix, width, author_style, news);
    }
    wrap_message_to_lines(
        body,
        stamp,
        prefix,
        width,
        author_style,
        body_style,
        mentions_us,
        continuation,
    )
}

// ── News formatting ─────────────────────────────────────────

#[derive(Debug, Clone)]
struct NewsPayload {
    title: String,
    summary: String,
    url: String,
    ascii_art: String,
}

fn parse_news_payload(body: &str) -> Option<NewsPayload> {
    let marker_pos = body.find(NEWS_MARKER)?;
    let raw = body[marker_pos + NEWS_MARKER.len()..].trim();
    if raw.is_empty() {
        return Some(NewsPayload {
            title: "news update".to_string(),
            summary: String::new(),
            url: String::new(),
            ascii_art: String::new(),
        });
    }

    let mut parts = raw.splitn(4, NEWS_SEPARATOR);
    let title = parts.next().unwrap_or_default().trim().to_string();
    let summary = parts.next().unwrap_or_default().trim().to_string();
    let url = parts.next().unwrap_or_default().trim().to_string();
    let ascii_art = decode_escaped_field(parts.next().unwrap_or_default().trim_end());

    Some(NewsPayload {
        title: if title.is_empty() {
            "news update".to_string()
        } else {
            title
        },
        summary,
        url,
        ascii_art,
    })
}

fn wrap_news_to_lines(
    stamp: &str,
    prefix: &str,
    width: usize,
    author_style: Style,
    payload: NewsPayload,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let border_style = Style::default().fg(theme::BORDER());
    let title_style = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let body_style = Style::default().fg(theme::CHAT_BODY());
    let meta_style = Style::default().fg(theme::TEXT_FAINT());

    let pad = Span::raw(" ");

    lines.push(Line::from(vec![
        pad.clone(),
        Span::styled(prefix.to_string(), author_style),
        Span::styled(" shared news ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(stamp.to_string(), meta_style),
    ]));

    if width < 10 {
        let fallback = format!(
            "{} | {} | {}",
            normalize_inline_text(&payload.title),
            normalize_inline_text(&payload.summary),
            normalize_inline_text(&payload.url)
        );
        lines.push(Line::from(vec![pad, Span::styled(fallback, body_style)]));
        return lines;
    }

    let inner_width = width.saturating_sub(2).max(1);
    let ascii_lines = raw_ascii_preview_lines(&payload.ascii_art, 6);
    let ascii_max_width = ascii_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(8)
        .max(8);
    let max_left_width = inner_width.saturating_sub(3 + 12).max(4);
    let left_width = ascii_max_width.min(14).min(max_left_width).max(4);
    let right_width = inner_width.saturating_sub(left_width + 3).max(1);

    let title = normalize_inline_text(&payload.title);
    let url = normalize_inline_text(&payload.url);

    let mut right_rows: Vec<(String, Style)> = Vec::new();
    if !title.is_empty() {
        for row in wrap_plain_line(&format!("📰 {title}"), right_width) {
            right_rows.push((row, title_style));
        }
    }
    if !payload.summary.is_empty() {
        for bullet in split_summary_bullets(&payload.summary) {
            let truncated = truncate_to_width(&bullet, right_width);
            right_rows.push((truncated, body_style));
        }
    }
    if !url.is_empty() {
        for row in wrap_plain_line(&url, right_width) {
            right_rows.push((row, meta_style));
        }
    }
    if right_rows.is_empty() {
        right_rows.push(("📰 news update".to_string(), title_style));
    }

    lines.push(Line::from(Span::styled(
        format!("┌{}┐", "─".repeat(inner_width)),
        border_style,
    )));

    let row_count = ascii_lines.len().max(right_rows.len()).max(1);
    for idx in 0..row_count {
        let left = ascii_lines.get(idx).map(String::as_str).unwrap_or("");
        let (right, right_style) = right_rows
            .get(idx)
            .map(|(text, style)| (text.as_str(), *style))
            .unwrap_or(("", body_style));
        lines.push(Line::from(vec![
            Span::styled("│", border_style),
            Span::styled(
                pad_to_width(left, left_width),
                Style::default().fg(theme::AMBER_DIM()),
            ),
            Span::styled(" │ ", border_style),
            Span::styled(pad_to_width(right, right_width), right_style),
            Span::styled("│", border_style),
        ]));
    }

    lines.push(Line::from(Span::styled(
        format!("└{}┘", "─".repeat(inner_width)),
        border_style,
    )));
    lines
}

// ── Text utilities ──────────────────────────────────────────

fn normalize_inline_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('•').trim_start_matches('-').trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_to_width(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    let mut out: String = chars.iter().take(width.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn split_summary_bullets(text: &str) -> Vec<String> {
    text.replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let stripped = line.trim_start_matches('•').trim_start_matches('-').trim();
            format!("• {stripped}")
        })
        .collect()
}

fn raw_ascii_preview_lines(ascii: &str, max_rows: usize) -> Vec<String> {
    let mut rows: Vec<String> = ascii
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .take(max_rows)
        .collect();
    if rows.is_empty() {
        rows.push("........".to_string());
    }
    rows
}

fn wrap_plain_line(text: &str, width: usize) -> Vec<String> {
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
        // Try to break at a space if we're not at the end
        let break_at = if end < chars.len() {
            // Look backwards for a space to break at
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

fn pad_to_width(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.chars().take(width).collect();
    }
    let mut out = String::with_capacity(width);
    out.push_str(text);
    out.push_str(&" ".repeat(width - len));
    out
}

fn decode_escaped_field(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

// ── Mention span highlighting ───────────────────────────────

pub(super) fn mention_spans(text: &str, body_style: Style) -> Vec<Span<'static>> {
    let mention_style = body_style.fg(theme::MENTION()).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut idx = 0;
    let mut segment_start = 0;

    while idx < text.len() {
        let Some(ch) = text[idx..].chars().next() else {
            break;
        };

        if ch == '@' && valid_mention_start(text, idx) {
            let mut end = idx + ch.len_utf8();
            let mut has_mention_chars = false;

            while end < text.len() {
                let Some(next) = text[end..].chars().next() else {
                    break;
                };
                if !is_mention_char(next) {
                    break;
                }
                has_mention_chars = true;
                end += next.len_utf8();
            }

            if has_mention_chars {
                if segment_start < idx {
                    spans.push(Span::styled(
                        text[segment_start..idx].to_string(),
                        body_style,
                    ));
                }
                spans.push(Span::styled(text[idx..end].to_string(), mention_style));
                idx = end;
                segment_start = end;
                continue;
            }
        }

        idx += ch.len_utf8();
    }

    if segment_start < text.len() {
        spans.push(Span::styled(
            text[segment_start..text.len()].to_string(),
            body_style,
        ));
    }

    spans
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

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
    fn mention_spans_highlight_mentions() {
        let spans = mention_spans("hey @alice and @bob", Style::default());
        assert_eq!(spans.len(), 4);
        assert_eq!(spans[0].content.as_ref(), "hey ");
        assert_eq!(spans[1].content.as_ref(), "@alice");
        assert_eq!(spans[2].content.as_ref(), " and ");
        assert_eq!(spans[3].content.as_ref(), "@bob");
        assert_eq!(spans[1].style.fg, Some(Color::Rgb(228, 196, 78)));
        assert_eq!(spans[3].style.fg, Some(Color::Rgb(228, 196, 78)));
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn mention_spans_ignore_email_addresses() {
        let spans = mention_spans("mail me at hi@example.com", Style::default());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "mail me at hi@example.com");
        assert_eq!(spans[0].style.fg, None);
    }

    #[test]
    fn mention_spans_stop_before_trailing_punctuation() {
        let spans = mention_spans("@alice, nice one", Style::default());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "@alice");
        assert_eq!(spans[1].content.as_ref(), ", nice one");
    }

    #[test]
    fn parse_news_payload_splits_marker_payload() {
        let body = "---NEWS--- Title || Summary line || https://example.com || .:-\\n+*#";
        let payload = parse_news_payload(body).expect("payload");
        assert_eq!(payload.title, "Title");
        assert_eq!(payload.summary, "Summary line");
        assert_eq!(payload.url, "https://example.com");
        assert_eq!(payload.ascii_art, ".:-\n+*#");
    }

    #[test]
    fn raw_ascii_preview_lines_limits_to_requested_rows() {
        let art = "abc\ndef\nghi\njkl";
        let lines = raw_ascii_preview_lines(art, 2);
        assert_eq!(lines, vec!["abc".to_string(), "def".to_string()]);
    }

    #[test]
    fn wrap_news_to_lines_renders_box_with_ascii_left() {
        let lines = wrap_news_to_lines(
            "[1m]",
            "mat: ",
            120,
            Style::default(),
            NewsPayload {
                title: "Title".to_string(),
                summary: "• first bullet".to_string(),
                url: "https://example.com".to_string(),
                ascii_art: ".:-\n+*#".to_string(),
            },
        );
        assert!(lines.len() >= 4);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("shared news"));
        assert!(rendered.contains("┌"));
        assert!(rendered.contains("└"));
        assert!(rendered.contains(".:-"));
        assert!(rendered.contains(" │ "));
        assert!(rendered.contains("Title"));
        assert!(rendered.contains("first bullet"));
        assert!(rendered.contains("https://example.com"));
    }

    // --- wrap_message_to_lines ---

    #[test]
    fn wrap_message_has_left_padding() {
        let lines = wrap_message_to_lines(
            "hello",
            "[1m]",
            "alice",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        // Author line starts with space
        assert!(strings[0].starts_with(" alice"));
        // Body line starts with space
        assert!(strings[1].starts_with(" hello"));
    }

    #[test]
    fn wrap_message_respects_newlines() {
        let lines = wrap_message_to_lines(
            "line1\nline2\nline3",
            "[1m]",
            "bob",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        // 1 author line + 3 body lines
        assert_eq!(strings.len(), 4);
        assert!(strings[1].contains("line1"));
        assert!(strings[2].contains("line2"));
        assert!(strings[3].contains("line3"));
    }

    #[test]
    fn wrap_message_empty_line_in_body() {
        let lines = wrap_message_to_lines(
            "above\n\nbelow",
            "[1m]",
            "mat",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        // author + "above" + empty + "below"
        assert_eq!(strings.len(), 4);
        assert!(strings[1].contains("above"));
        assert_eq!(strings[2].trim(), "");
        assert!(strings[3].contains("below"));
    }

    #[test]
    fn wrap_message_wraps_long_lines() {
        let lines = wrap_message_to_lines(
            "abcdefghij",
            "[1m]",
            "x",
            6, // area width 6, minus 1 pad = 5 body cols
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        // author + 2 wrapped body lines
        assert_eq!(strings.len(), 3);
        assert!(strings[1].contains("abcde"));
        assert!(strings[2].contains("fghij"));
    }

    #[test]
    fn wrap_message_prefers_word_boundaries() {
        let lines = wrap_message_to_lines(
            "hello wide world",
            "[1m]",
            "x",
            8,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        assert_eq!(strings.len(), 4);
        assert!(strings[1].contains("hello"));
        assert!(strings[2].contains("wide"));
        assert!(strings[3].contains("world"));
    }

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }

    #[test]
    fn wrap_message_renders_reply_quote_separately() {
        let lines = wrap_message_to_lines(
            "> @alice: original text\nmy reply",
            "[1m]",
            "bob",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        let strings = lines_to_strings(&lines);
        assert_eq!(strings.len(), 3);
        assert!(strings[1].contains("> @alice: original text"));
        assert!(strings[2].contains("my reply"));
    }

    #[test]
    fn wrap_message_empty_body() {
        let lines = wrap_message_to_lines(
            "",
            "[1m]",
            "alice",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
        );
        // Only author line
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn wrap_plain_line_preserves_leading_spaces() {
        let result = wrap_plain_line("   hello", 40);
        assert_eq!(result, vec!["   hello"]);
    }

    #[test]
    fn wrap_plain_line_preserves_ascii_art() {
        let art = "  .@@@@@@.";
        let result = wrap_plain_line(art, 40);
        assert_eq!(result, vec!["  .@@@@@@."]);
    }

    #[test]
    fn wrap_plain_line_preserves_internal_spacing() {
        let result = wrap_plain_line("a    b    c", 40);
        assert_eq!(result, vec!["a    b    c"]);
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

    #[test]
    fn wrap_plain_line_whitespace_only_returns_empty() {
        let result = wrap_plain_line("   ", 40);
        assert!(result.is_empty());
    }
}
