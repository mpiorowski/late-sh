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

pub struct FeedListView<'a> {
    pub entries: &'a [RssEntryView],
    pub selected_index: usize,
    pub has_feeds: bool,
    pub marker_read_at: Option<DateTime<Utc>>,
}

const ITEM_HEIGHT: u16 = 7;
const SUMMARY_LINES: usize = 2;

pub fn draw_feed_list(frame: &mut Frame, area: Rect, view: &FeedListView<'_>) {
    let selected = if view.entries.is_empty() {
        0
    } else {
        view.selected_index.min(view.entries.len() - 1) + 1
    };
    let title = format!(" Feeds ({selected}/{}) ", view.entries.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.entries.is_empty() {
        let text = if view.has_feeds {
            "No feed entries yet. Press r to refresh."
        } else {
            "No feeds connected. Add RSS/Atom URLs in Settings > Feeds."
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
            Paragraph::new(entry_lines(item, is_unread)).wrap(Wrap { trim: true }),
            content,
        );
    }
}

fn entry_lines(item: &RssEntryView, is_unread: bool) -> Vec<Line<'static>> {
    let mut title_spans = Vec::new();
    if is_unread {
        title_spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    title_spans.push(Span::styled(
        item.entry.title.clone(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ));

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

    for line in item
        .entry
        .summary
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(SUMMARY_LINES)
    {
        lines.push(Line::from(Span::styled(
            line.trim().to_string(),
            Style::default().fg(theme::TEXT()),
        )));
    }
    lines
}

fn display_feed_title(item: &RssEntryView) -> String {
    let title = item.feed_title.trim();
    if title.is_empty() {
        item.feed_url.clone()
    } else {
        title.to_string()
    }
}
