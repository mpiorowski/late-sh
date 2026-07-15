use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::chat::action::parse_action_body;
use crate::app::chat::svc::ReportKind;
use crate::app::common::username_effect::{NameStyle, char_color};
use crate::app::common::{markdown::render_body_to_lines, theme};
use late_core::models::{article::NEWS_MARKER, chat_message_reaction::ChatMessageReactionSummary};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NEWS_SEPARATOR: &str = " || ";

/// The flair painted over the bare username inside the author header:
/// the tavern drunk glow as a background tint and/or a bought 24h username
/// effect as per-character foreground color. `range` is the username's byte
/// range within the prefix string, so badges and flags stay untouched.
/// `word` is the drunk state printed after the header (e.g. "wasted"),
/// present only once the drinker is soused enough to earn a label; the glow
/// alone carries lighter states. The effect fg deliberately overrides the
/// base author fg (own amber, friend gold, default) while keeping its bg and
/// modifiers, so a bought effect always wins the color of the name.
#[derive(Clone, Copy, Debug)]
pub(super) struct AuthorTint {
    pub range: (usize, usize),
    pub bg: Option<Color>,
    pub word: Option<&'static str>,
    pub name_style: Option<NameStyle>,
}

/// The trailing ` (word)` span appended after the author header for a drinker
/// deep enough to warrant a printed label. Faint and italic so it reads as an
/// aside next to the name, not another badge.
fn drunk_word_span(word: &str) -> Span<'static> {
    Span::styled(
        format!(" ({word})"),
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    )
}

/// The author header's prefix spans: one span when untinted (byte-identical
/// to the historical output), split when drunk tint and/or a username effect
/// paints the name. Falls back to the single span on any out-of-bounds range.
///
/// A username effect emits one span per character so gradients and shimmer
/// interpolate across the name; the country-flag emoji inside the range
/// ignores fg color, which is fine — the readable characters carry the look.
fn push_author_prefix_spans(
    spans: &mut Vec<Span<'static>>,
    prefix: &str,
    author_style: Style,
    tint: Option<AuthorTint>,
) {
    if let Some(tint) = tint {
        let (start, end) = tint.range;
        if start < end
            && end <= prefix.len()
            && prefix.is_char_boundary(start)
            && prefix.is_char_boundary(end)
        {
            if start > 0 {
                spans.push(Span::styled(prefix[..start].to_string(), author_style));
            }
            let name = &prefix[start..end];
            let name_base = match tint.bg {
                Some(bg) => author_style.bg(bg),
                None => author_style,
            };
            match tint.name_style {
                Some(style) => {
                    let len = name.chars().count();
                    spans.extend(name.chars().enumerate().map(|(index, ch)| {
                        Span::styled(ch.to_string(), name_base.fg(char_color(style, index, len)))
                    }));
                }
                None => spans.push(Span::styled(name.to_string(), name_base)),
            }
            if end < prefix.len() {
                spans.push(Span::styled(prefix[end..].to_string(), author_style));
            }
            return;
        }
    }
    spans.push(Span::styled(prefix.to_string(), author_style));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wrap_message_to_lines(
    body: &str,
    stamp: &str,
    prefix: &str,
    width: usize,
    author_style: Style,
    author_tint: Option<AuthorTint>,
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
        let mut spans = vec![pad.clone()];
        push_author_prefix_spans(&mut spans, prefix, author_style, author_tint);
        if let Some(word) = author_tint.and_then(|tint| tint.word) {
            spans.push(drunk_word_span(word));
        }
        spans.push(Span::styled(
            format!(" {stamp}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        lines.push(Line::from(spans));
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
    author_tint: Option<AuthorTint>,
    body_style: Style,
    mentions_us: bool,
    continuation: bool,
    system_text: Option<&str>,
    inline_image_lines: Option<&[Line<'static>]>,
    reactions: &[ChatMessageReactionSummary],
) -> WrappedChatEntry {
    let pad = if mentions_us {
        Span::styled("│", Style::default().fg(theme::MENTION()))
    } else {
        Span::raw(" ")
    };
    let news_payload = system_text
        .is_none()
        .then(|| parse_news_payload(body))
        .flatten();
    let report_payload = (system_text.is_none() && news_payload.is_none())
        .then(|| parse_report_payload(body))
        .flatten();
    let action_payload =
        (system_text.is_none() && news_payload.is_none() && report_payload.is_none())
            .then(|| parse_action_body(body))
            .flatten();
    // Only normal (non-news, non-report, non-system), non-continuation
    // messages emit a clickable author header for mouse hit-testing — news
    // and report cards have their own card layout, system lines are
    // authorless, and continuation messages omit the header so a run reads
    // as one block.
    let header_line_index = (system_text.is_none()
        && news_payload.is_none()
        && report_payload.is_none()
        && action_payload.is_none()
        && !continuation)
        .then_some(0);
    let mut lines = if let Some(system) = system_text {
        wrap_system_to_lines(system, width)
    } else if let Some(news) = news_payload {
        wrap_news_to_lines(stamp, prefix, width, author_style, news)
    } else if let Some((kind, text)) = report_payload {
        wrap_report_to_lines(stamp, prefix, width, author_style, kind, text)
    } else if let Some(action) = action_payload {
        wrap_action_to_lines(action, prefix, width, body_style, mentions_us)
    } else {
        wrap_message_to_lines(
            body,
            stamp,
            prefix,
            width,
            author_style,
            author_tint,
            body_style,
            mentions_us,
            continuation,
        )
    };

    let image_line_range = if let Some(img_lines) = inline_image_lines.filter(|l| !l.is_empty()) {
        let start = lines.len();
        for img_line in img_lines {
            let mut spans = vec![pad.clone(), Span::raw(" ")];
            spans.extend(img_line.spans.iter().cloned());
            lines.push(Line::from(spans));
        }
        Some((start, lines.len()))
    } else {
        None
    };

    lines.extend(render_reaction_footer_lines(reactions, width, pad));
    WrappedChatEntry {
        lines,
        header_line_index,
        image_line_range,
    }
}

/// A #lounge system-feed line (see `activity/lounge.rs`). The prefix alone
/// is NOT trusted: callers must also check the author is the system user
/// before styling, so neither a human named "system" nor a pasted "· " can
/// spoof the authorless row.
pub(crate) fn parse_system_line(body: &str) -> Option<&str> {
    let text = body
        .strip_prefix(crate::app::activity::lounge::SYSTEM_LINE_PREFIX)?
        .trim();
    (!text.is_empty()).then_some(text)
}

/// System lines render as exactly one authorless row — a stacked run must
/// stay dense — so overlong text is truncated, never wrapped.
fn wrap_system_to_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let budget = width.saturating_sub(4); // pad + "· " + right breathing room
    let shown: String = if text.chars().count() > budget && budget > 1 {
        let mut cut: String = text.chars().take(budget - 1).collect();
        cut.push('…');
        cut
    } else {
        text.to_string()
    };
    vec![Line::from(vec![
        Span::raw(" "),
        Span::styled("· ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            shown,
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        ),
    ])]
}

fn wrap_action_to_lines(
    action: &str,
    prefix: &str,
    width: usize,
    body_style: Style,
    mentions_us: bool,
) -> Vec<Line<'static>> {
    let pad = if mentions_us {
        Span::styled("│", Style::default().fg(theme::MENTION()))
    } else {
        Span::raw(" ")
    };
    let style = body_style.add_modifier(Modifier::ITALIC);
    render_body_to_lines(&format!("* {prefix} {action}"), width, pad, style)
}

pub(super) struct WrappedChatEntry {
    pub lines: Vec<Line<'static>>,
    /// Index of the author/header line within `lines`, if present. Absent
    /// for news cards (different layout) and for continuation messages
    /// (header intentionally omitted so a run reads as one block).
    pub header_line_index: Option<usize>,
    /// Half-open range `[start, end)` of inline-image rows within `lines`.
    /// `None` when the message has no inline image preview.
    pub image_line_range: Option<(usize, usize)>,
}

// ── News formatting ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewsPayload {
    pub title: String,
    pub summary: String,
    pub url: String,
    pub ascii_art: String,
}

pub(crate) fn parse_news_payload(body: &str) -> Option<NewsPayload> {
    let raw = body.trim_start().strip_prefix(NEWS_MARKER)?.trim();
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

pub(crate) fn format_news_ascii_art_for_display(ascii: &str, max_rows: usize) -> Vec<String> {
    if max_rows == 0 {
        return Vec::new();
    }

    ascii
        .replace("\\n", "\n")
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .take(max_rows)
        .collect()
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
    let mut ascii_lines = format_news_ascii_art_for_display(&payload.ascii_art, 6);
    if ascii_lines.is_empty() {
        ascii_lines.push("........".to_string());
    }
    let ascii_max_width = ascii_lines
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
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
        for row in wrap_plain_display_width(&format!("📰 {title}"), right_width) {
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
        for row in wrap_plain_display_width(&url, right_width) {
            right_rows.push((row, meta_style));
        }
    }
    if right_rows.is_empty() {
        right_rows.push(("📰 news update".to_string(), title_style));
    }

    lines.push(Line::from(vec![
        pad.clone(),
        Span::styled("─".repeat(inner_width), border_style),
    ]));

    let row_count = ascii_lines.len().max(right_rows.len()).max(1);
    for idx in 0..row_count {
        let left = ascii_lines.get(idx).map(String::as_str).unwrap_or("");
        let (right, right_style) = right_rows
            .get(idx)
            .map(|(text, style)| (text.as_str(), *style))
            .unwrap_or(("", body_style));
        lines.push(Line::from(vec![
            pad.clone(),
            Span::styled(
                pad_to_display_width(left, left_width),
                Style::default().fg(theme::AMBER_DIM()),
            ),
            Span::styled(" │ ", border_style),
            Span::styled(pad_to_display_width(right, right_width), right_style),
        ]));
    }
    lines.push(Line::from(vec![
        pad,
        Span::styled("─".repeat(inner_width), border_style),
    ]));
    lines
}

// ── Report cards (/bug, /suggest) ───────────────────────────

/// A `/bug` or `/suggest` report card: the kind's marker at the start of the
/// body, everything after it is the report text. Mirrors `parse_news_payload`:
/// the marker must open the message so pasted markers mid-text don't spoof a
/// card.
pub(crate) fn parse_report_payload(body: &str) -> Option<(ReportKind, &str)> {
    let trimmed = body.trim_start();
    for kind in [ReportKind::Bug, ReportKind::Suggestion] {
        if let Some(rest) = trimmed.strip_prefix(kind.marker()) {
            return Some((kind, rest.trim()));
        }
    }
    None
}

/// Report cards render as a compact ruled block so reports stand apart from
/// staff replies in the report-only rooms:
/// ```text
///  mat filed a bug 12:34
///  ────────────────────
///  🐛 the thing broke when …
///  ────────────────────
/// ```
fn wrap_report_to_lines(
    stamp: &str,
    prefix: &str,
    width: usize,
    author_style: Style,
    kind: ReportKind,
    text: &str,
) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(theme::BORDER());
    let body_style = Style::default().fg(theme::CHAT_BODY());
    let meta_style = Style::default().fg(theme::TEXT_FAINT());
    let pad = Span::raw(" ");

    let mut lines = vec![Line::from(vec![
        pad.clone(),
        Span::styled(prefix.to_string(), author_style),
        Span::styled(
            format!(" {} ", kind.verb()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(stamp.to_string(), meta_style),
    ])];

    let text = if text.is_empty() {
        kind.command()
    } else {
        text
    };
    let body = format!("{} {}", kind.icon(), text);
    if width < 10 {
        lines.push(Line::from(vec![
            pad,
            Span::styled(normalize_inline_text(&body), body_style),
        ]));
        return lines;
    }

    let inner_width = width.saturating_sub(2).max(1);
    let rule = || {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("─".repeat(inner_width), border_style),
        ])
    };
    lines.push(rule());
    lines.extend(render_body_to_lines(&body, width, pad, body_style));
    lines.push(rule());
    lines
}

// ── Reaction footer ─────────────────────────────────────────

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
        let text = format!("[{} {}]", reaction.icon, reaction.count);
        let chip_width = UnicodeWidthStr::width(text.as_str());
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
        6 => "🙌",
        7 => "🚀",
        8 => "🤔",
        9 => "💩",
        0 => "👋",
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
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width <= 3 {
        return ".".repeat(width);
    }

    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width.saturating_sub(3) {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push_str("...");
    out
}

fn pad_to_display_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}

fn wrap_plain_display_width(text: &str, width: usize) -> Vec<String> {
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
        let mut end = idx;
        let mut used = 0;
        while end < chars.len() {
            let ch_width = UnicodeWidthChar::width(chars[end]).unwrap_or(0);
            if used > 0 && used + ch_width > width {
                break;
            }
            used += ch_width;
            end += 1;
            if used >= width {
                break;
            }
        }

        let break_at = if end < chars.len() {
            let mut pos = end;
            while pos > idx && chars[pos - 1] != ' ' {
                pos -= 1;
            }
            if pos > idx { pos } else { end.max(idx + 1) }
        } else {
            end
        };
        out.push(chars[idx..break_at].iter().collect());
        idx = break_at;
        while idx < chars.len() && chars[idx] == ' ' {
            idx += 1;
        }
    }
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
    fn parse_news_payload_requires_marker_at_start() {
        assert!(parse_news_payload("hello ---NEWS--- Fake || summary || url || ascii").is_none());
        assert!(parse_news_payload("  ---NEWS--- Title || Summary || url || ascii").is_some());
    }

    #[test]
    fn parse_report_payload_requires_marker_at_start() {
        assert_eq!(
            parse_report_payload("---BUG--- the door ate my hat"),
            Some((ReportKind::Bug, "the door ate my hat"))
        );
        assert_eq!(
            parse_report_payload("  ---SUGGESTION--- more cats"),
            Some((ReportKind::Suggestion, "more cats"))
        );
        assert!(parse_report_payload("hello ---BUG--- fake").is_none());
        assert!(parse_report_payload("regular message").is_none());
    }

    #[test]
    fn wrap_chat_entry_to_lines_renders_report_card() {
        let wrapped = wrap_chat_entry_to_lines(
            "---BUG--- the door ate my hat",
            "[now]",
            "mat",
            40,
            Style::default(),
            None,
            Style::default(),
            false,
            false,
            None,
            None,
            &[],
        );
        let lines = lines_to_strings(&wrapped.lines);
        assert_eq!(lines[0], " mat filed a bug [now]");
        assert!(lines[1].contains('─'), "{lines:?}");
        assert!(lines[2].contains("🐛 the door ate my hat"), "{lines:?}");
        assert!(lines.last().unwrap().contains('─'), "{lines:?}");
        assert_eq!(wrapped.header_line_index, None);
    }

    #[test]
    fn wrap_chat_entry_to_lines_renders_action_message() {
        let body = crate::app::chat::action::encode_action_body("waves").expect("action");
        let wrapped = wrap_chat_entry_to_lines(
            &body,
            "[now]",
            "mat",
            80,
            Style::default(),
            None,
            Style::default(),
            false,
            false,
            None,
            None,
            &[],
        );
        assert_eq!(lines_to_strings(&wrapped.lines), vec![" * mat waves"]);
        assert_eq!(wrapped.header_line_index, None);
    }

    #[test]
    fn format_news_ascii_art_for_display_limits_to_requested_rows() {
        let art = "abc\ndef\nghi\njkl";
        let lines = format_news_ascii_art_for_display(art, 2);
        assert_eq!(lines, vec!["abc".to_string(), "def".to_string()]);
    }

    #[test]
    fn format_news_ascii_art_for_display_drops_blank_rows_and_trims_right_edge() {
        let art = "\n   \n  abc  \n\\n def\t \n";
        let lines = format_news_ascii_art_for_display(art, 6);
        assert_eq!(lines, vec!["  abc".to_string(), " def".to_string()]);
    }

    #[test]
    fn format_news_ascii_art_for_display_allows_short_or_empty_art() {
        assert_eq!(
            format_news_ascii_art_for_display("one\n\n", 6),
            vec!["one".to_string()]
        );
        assert!(format_news_ascii_art_for_display("\n  \n", 6).is_empty());
        assert!(format_news_ascii_art_for_display("one", 0).is_empty());
    }

    #[test]
    fn wrap_news_to_lines_renders_rules_with_ascii_left() {
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
        for row in lines_to_strings(&lines) {
            assert!(
                row.starts_with(' '),
                "custom card row lost left padding: {row:?}"
            );
        }
        assert!(rendered.contains("shared news"));
        assert!(!rendered.contains("┌"));
        assert!(!rendered.contains("┐"));
        assert!(!rendered.contains("└"));
        assert!(!rendered.contains("┘"));
        assert!(rendered.contains("──"));
        assert!(
            rendered
                .lines()
                .filter(|line| line.trim().chars().all(|ch| ch == '─'))
                .count()
                >= 2
        );
        assert!(rendered.contains(".:-"));
        assert!(rendered.contains(" │ "));
        assert!(rendered.contains("Title"));
        assert!(rendered.contains("first bullet"));
        assert!(rendered.contains("https://example.com"));
    }

    #[test]
    fn wrap_news_to_lines_respects_terminal_cell_width() {
        let width = 58;
        let lines = wrap_news_to_lines(
            "[4 mins ago]",
            "@artboard",
            width,
            Style::default(),
            NewsPayload {
                title: "Nobody understands the point of hybrid cars".to_string(),
                summary:
                    "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
                        .to_string(),
                url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
                ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .".to_string(),
            },
        );

        for rendered in lines_to_strings(&lines) {
            assert!(
                UnicodeWidthStr::width(rendered.as_str()) <= width,
                "line overflowed {width} cells: {rendered:?}"
            );
        }
    }

    #[test]
    fn wrap_chat_entry_to_lines_appends_reaction_footer() {
        let wrapped = wrap_chat_entry_to_lines(
            "hello world",
            "[1m]",
            "alice",
            80,
            Style::default(),
            None,
            Style::default(),
            false,
            false,
            None,
            None,
            &[
                ChatMessageReactionSummary {
                    icon: "🧡".to_string(),
                    count: 3,
                },
                ChatMessageReactionSummary {
                    icon: "🔥".to_string(),
                    count: 1,
                },
            ],
        );
        let rendered = lines_to_strings(&wrapped.lines).join("\n");
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
            None,
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
            None,
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
            None,
            Style::default(),
            false,
            false,
        );
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn wrap_message_author_tint_splits_only_the_username() {
        let tint = AuthorTint {
            range: (4, 9), // "alice" inside "★ alice 🌱" ("★" is 3 bytes)
            bg: Some(Color::Rgb(10, 20, 30)),
            word: None,
            name_style: None,
        };
        let lines = wrap_message_to_lines(
            "hello",
            "[1m]",
            "★ alice 🌱",
            80,
            Style::default(),
            Some(tint),
            Style::default(),
            false,
            false,
        );
        // pad + prefix-before + tinted-username + prefix-after + stamp
        let header = &lines[0];
        assert_eq!(header.spans.len(), 5);
        assert_eq!(header.spans[2].content.as_ref(), "alice");
        assert_eq!(header.spans[2].style.bg, Some(Color::Rgb(10, 20, 30)));
        assert_eq!(header.spans[1].style.bg, None);
        assert_eq!(header.spans[3].style.bg, None);
        // Text is identical to the untinted render.
        let untinted = wrap_message_to_lines(
            "hello",
            "[1m]",
            "★ alice 🌱",
            80,
            Style::default(),
            None,
            Style::default(),
            false,
            false,
        );
        assert_eq!(lines_to_strings(&lines), lines_to_strings(&untinted));
    }

    #[test]
    fn wrap_message_author_tint_ignores_bad_ranges() {
        let tint = AuthorTint {
            range: (0, 99),
            bg: Some(Color::Rgb(10, 20, 30)),
            word: None,
            name_style: None,
        };
        let lines = wrap_message_to_lines(
            "hello",
            "[1m]",
            "alice",
            80,
            Style::default(),
            Some(tint),
            Style::default(),
            false,
            false,
        );
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[1].style.bg, None);
    }

    #[test]
    fn wrap_message_name_style_paints_per_char_over_drunk_bg() {
        let tint = AuthorTint {
            range: (0, 5),
            bg: Some(Color::Rgb(10, 20, 30)),
            word: None,
            name_style: Some(NameStyle::Solid(Color::Rgb(255, 200, 80))),
        };
        let author_style = Style::default()
            .fg(Color::Rgb(1, 2, 3))
            .add_modifier(Modifier::BOLD);
        let lines = wrap_message_to_lines(
            "hello",
            "12:04",
            "alice",
            80,
            author_style,
            Some(tint),
            Style::default(),
            false,
            false,
        );
        // pad + 5 per-char spans + stamp
        let header = &lines[0];
        assert_eq!(header.spans.len(), 7);
        let name: String = header.spans[1..6]
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(name, "alice");
        for span in &header.spans[1..6] {
            // Effect fg wins over the author fg; drunk bg and BOLD survive.
            assert_eq!(span.style.fg, Some(Color::Rgb(255, 200, 80)));
            assert_eq!(span.style.bg, Some(Color::Rgb(10, 20, 30)));
            assert!(span.style.add_modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn wrap_message_prints_drunk_word_between_name_and_stamp() {
        let tint = AuthorTint {
            range: (0, 5),
            bg: Some(Color::Rgb(10, 20, 30)),
            word: Some("wasted"),
            name_style: None,
        };
        let lines = wrap_message_to_lines(
            "hello",
            "12:04",
            "alice",
            80,
            Style::default(),
            Some(tint),
            Style::default(),
            false,
            false,
        );
        // pad + tinted-username + " (wasted)" + " 12:04"
        let header = &lines[0];
        assert_eq!(header.spans.len(), 4);
        assert_eq!(header.spans[2].content.as_ref(), " (wasted)");
        assert!(
            header.spans[2]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert_eq!(header.spans[3].content.as_ref(), " 12:04");
    }

    #[test]
    fn wrap_message_omits_drunk_word_when_absent() {
        // The glow can be present with no word (light buzz): header stays lean.
        let tint = AuthorTint {
            range: (0, 5),
            bg: Some(Color::Rgb(10, 20, 30)),
            word: None,
            name_style: None,
        };
        let lines = wrap_message_to_lines(
            "hello",
            "12:04",
            "alice",
            80,
            Style::default(),
            Some(tint),
            Style::default(),
            false,
            false,
        );
        // pad + tinted-username + " 12:04" — no aside.
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[2].content.as_ref(), " 12:04");
    }

    #[test]
    fn composer_rows_soft_wrap_words() {
        let rows = build_composer_rows("hello wide world", 8);
        let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "wide", "world"]);
    }
}
