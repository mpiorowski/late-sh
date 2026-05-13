use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;
use chrono::{DateTime, Utc};
use late_core::models::rss_entry::RssEntryView;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub struct FeedListView<'a> {
    pub entries: &'a [RssEntryView],
    pub selected_index: usize,
    pub has_feeds: bool,
    pub marker_read_at: Option<DateTime<Utc>>,
}

const ITEM_HEIGHT: u16 = 7;
const SUMMARY_MAX_CHARS: usize = 240;

pub fn draw_feed_list(frame: &mut Frame, area: Rect, view: &FeedListView<'_>) {
    let inner = area;

    if view.entries.is_empty() {
        let text = if view.has_feeds {
            "No RSS entries yet. Press r to refresh."
        } else {
            "No RSS sources connected. Add RSS/Atom URLs in Settings > RSS."
        };
        frame.render_widget(
            Paragraph::new(Text::from(text)).style(Style::default().fg(theme::TEXT_DIM())),
            inner,
        );
        return;
    }

    let visible_items = ((inner.height / ITEM_HEIGHT).max(1)) as usize;
    let selected_index = view
        .selected_index
        .min(view.entries.len().saturating_sub(1));
    let start = selected_index.saturating_sub(visible_items.saturating_sub(1));
    let end = (start + visible_items).min(view.entries.len());
    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), end - start).collect::<Vec<_>>();
    let rows = Layout::vertical(constraints).split(inner);

    for (row, area) in rows.iter().copied().enumerate() {
        let idx = start + row;
        let item = &view.entries[idx];
        let selected = idx == selected_index;
        let bg = if selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };
        let item_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(Style::default().bg(bg));
        let content = item_block.inner(area);
        frame.render_widget(item_block, area);
        let is_unread = view
            .marker_read_at
            .map(|last_read_at| item.entry.created > last_read_at)
            .unwrap_or(true);
        frame.render_widget(
            Paragraph::new(entry_lines(item, is_unread, content.width as usize))
                .wrap(Wrap { trim: true }),
            content,
        );
    }
}

fn entry_lines(item: &RssEntryView, is_unread: bool, width: usize) -> Vec<Line<'static>> {
    let mut title_spans = Vec::with_capacity(4);
    if is_unread {
        title_spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let unread_w = if is_unread {
        UnicodeWidthStr::width("● ")
    } else {
        0
    };
    let shared = item.entry.shared_at.is_some();
    let badge = if shared { "(shared)" } else { "" };
    let badge_w = UnicodeWidthStr::width(badge);
    let title_budget = if shared {
        width
            .saturating_sub(unread_w)
            .saturating_sub(badge_w + 1)
            .max(4)
    } else {
        width.saturating_sub(unread_w).max(4)
    };
    let title = truncate_to_width(&item.entry.title, title_budget);
    let title_w = UnicodeWidthStr::width(title.as_str());
    title_spans.push(Span::styled(
        title,
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ));
    if shared {
        let used = unread_w + title_w;
        let pad = width.saturating_sub(used + badge_w).max(1);
        title_spans.push(Span::raw(" ".repeat(pad)));
        title_spans.push(Span::styled(
            badge,
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }

    let mut lines = vec![
        Line::from(title_spans),
        Line::from(Span::styled(
            item.entry.url.clone(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(vec![
            Span::styled(
                display_feed_title(item),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                item.entry
                    .published_at
                    .map(|dt| format!(" - {}", format_relative_time(dt)))
                    .unwrap_or_default(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];

    let summary = item.entry.summary.trim();
    if !summary.is_empty() {
        lines.push(Line::from(Span::styled(
            truncate_summary(summary, SUMMARY_MAX_CHARS),
            Style::default().fg(theme::TEXT()),
        )));
    }
    lines
}

/// Cap a summary at `max_chars`, breaking at the last whitespace within
/// the final 30 chars of the budget so we don't slice mid-word. Strips
/// trailing punctuation before the ellipsis so `... .` doesn't render.
fn truncate_summary(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let look_back = max_chars.saturating_sub(30);
    let cut = (look_back..max_chars)
        .rev()
        .find(|i| chars[*i].is_whitespace())
        .unwrap_or(max_chars);
    let mut out: String = chars[..cut].iter().collect();
    out = out
        .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':'))
        .to_string();
    out.push('…');
    out
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }

    let ellipsis_w = 1;
    let budget = max_width.saturating_sub(ellipsis_w);
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn display_feed_title(item: &RssEntryView) -> String {
    let title = item.feed_title.trim();
    if title.is_empty() {
        item.feed_url.clone()
    } else {
        title.to_string()
    }
}
