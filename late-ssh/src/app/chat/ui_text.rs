use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::{
    markdown::{pad_to_width, render_body_to_lines, wrap_plain_line},
    theme,
};
use late_core::models::{article::NEWS_MARKER, chat_message_reaction::ChatMessageReactionSummary};

const NEWS_SEPARATOR: &str = " || ";

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
        lines.push(Line::from(vec![
            pad.clone(),
            Span::styled(prefix.to_string(), author_style),
            Span::styled(
                format!(" {stamp}"),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    }

    if body.is_empty() {
        return lines;
    }

    lines.extend(render_body_to_lines(body, width, pad, body_style));

    lines
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
    reactions: &[ChatMessageReactionSummary],
) -> Vec<Line<'static>> {
    let pad = if mentions_us {
        Span::styled("│", Style::default().fg(theme::MENTION()))
    } else {
        Span::raw(" ")
    };
    let mut lines = if let Some(news) = parse_news_payload(body) {
        wrap_news_to_lines(stamp, prefix, width, author_style, news)
    } else {
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
    };
    lines.extend(render_reaction_footer_lines(reactions, width, pad));
    lines
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

fn render_reaction_footer_lines(
    reactions: &[ChatMessageReactionSummary],
    width: usize,
    pad: Span<'static>,
) -> Vec<Line<'static>> {
    if reactions.is_empty() {
        return Vec::new();
    }

    let mut footer_lines: Vec<Line<'static>> = Vec::new();
    let available_width = width.saturating_sub(1).max(1);
    let mut current_width = 0usize;
    let mut current_spans = vec![pad.clone()];

    for reaction in reactions {
        let text = format!("[{} {}]", reaction_label(reaction.kind), reaction.count);
        let chip_width = text.chars().count();
        let extra_space = usize::from(current_width > 0);
        if current_width > 0 && current_width + extra_space + chip_width > available_width {
            footer_lines.push(Line::from(current_spans));
            current_spans = vec![pad.clone()];
            current_width = 0;
        }
        if current_width > 0 {
            current_spans.push(Span::raw(" "));
            current_width += 1;
        }
        current_spans.push(Span::styled(text, Style::default().fg(theme::TEXT_DIM())));
        current_width += chip_width;
    }

    footer_lines.push(Line::from(current_spans));
    footer_lines
}

pub(super) fn reaction_label(kind: i16) -> &'static str {
    match kind {
        1 => "👍",
        2 => "🧡",
        3 => "😂",
        4 => "👀",
        5 => "🔥",
        _ => "?",
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::common::composer::build_composer_rows;
    use late_core::models::chat_message_reaction::ChatMessageReactionSummary;

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

    #[test]
    fn wrap_chat_entry_to_lines_appends_reaction_footer() {
        let lines = wrap_chat_entry_to_lines(
            "hello world",
            "[1m]",
            "alice",
            80,
            Style::default(),
            Style::default(),
            false,
            false,
            &[
                ChatMessageReactionSummary { kind: 2, count: 3 },
                ChatMessageReactionSummary { kind: 5, count: 1 },
            ],
        );
        let rendered = lines_to_strings(&lines).join("\n");
        assert!(rendered.contains("[🧡 3]"));
        assert!(rendered.contains("[🔥 1]"));
    }

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
        assert!(strings[0].starts_with(" alice"));
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
        assert_eq!(strings.len(), 4);
        assert!(strings[1].contains("line1"));
        assert!(strings[2].contains("line2"));
        assert!(strings[3].contains("line3"));
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
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }
}
