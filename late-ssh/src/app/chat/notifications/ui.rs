use crate::app::common::text_layout;

use std::collections::HashSet;

use crate::app::common::{primitives::format_relative_time, theme};
use late_core::models::notification::NotificationView;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use uuid::Uuid;

pub struct NotificationListView<'a> {
    pub items: &'a [NotificationView],
    pub selected_index: usize,
    /// What this visit stamped read: their dots stay up until the next visit.
    pub read_on_open: &'a HashSet<Uuid>,
}

const ITEM_HEIGHT: u16 = 4;
const PREVIEW_ROWS: usize = 2;

pub fn draw_notification_list(frame: &mut Frame, area: Rect, view: &NotificationListView<'_>) {
    let inner_area = area;

    if view.items.is_empty() {
        let text = Text::from("No mentions yet.");
        let p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(p, inner_area);
        return;
    }

    let visible_items = (inner_area.height / ITEM_HEIGHT).max(1) as usize;
    let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
    let start_index = selected_index.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(view.items.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (row, item_area) in layout.iter().copied().enumerate() {
        let idx = start_index + row;
        let item = &view.items[idx];

        let item_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(theme::row_style(idx == selected_index));

        let content_area = item_block.inner(item_area);
        frame.render_widget(item_block, item_area);

        let room_label = item
            .room_slug
            .as_deref()
            .map(|s| format!("#{s}"))
            .unwrap_or_else(|| "DM".to_string());

        // Unread, or read by this very visit: opening the list stamps its
        // mentions, and the dots survive the visit that read them.
        let is_new = item.read_at.is_none() || view.read_on_open.contains(&item.id);
        let read_indicator = if is_new {
            Span::styled("● ", Style::default().fg(theme::MENTION()))
        } else {
            Span::styled(" ", Style::default())
        };

        let mut lines = vec![Line::from(vec![
            read_indicator,
            Span::styled(
                format!("@{}", item.actor_username),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" mentioned you in {room_label}"),
                Style::default().fg(theme::TEXT()),
            ),
            Span::styled(
                format!("  {}", format_relative_time(item.created)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])];

        let preview_width = content_area.width.saturating_sub(2) as usize;
        let preview_rows = preview_rows(
            &mention_preview(&item.message_body),
            preview_width,
            PREVIEW_ROWS,
        );
        lines.extend(preview_rows.into_iter().map(|row| {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(row, Style::default().fg(theme::TEXT_FAINT())),
            ])
        }));

        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(p, content_area);
    }
}

/// How much of a mentioning message the list reads.
const MENTION_PREVIEW_CHARS: usize = 120;

/// A mention's preview: `||spoilers||` masked first, then cut, so the cut
/// can never split one and leave its first words in clear.
fn mention_preview(body: &str) -> String {
    crate::app::common::spoiler::mask_spoilers(body)
        .chars()
        .take(MENTION_PREVIEW_CHARS)
        .collect()
}

fn preview_rows(body: &str, width: usize, max_rows: usize) -> Vec<String> {
    let width = width.max(1);
    // When a reply quotes the message it answers ("> ..."), show only the
    // reply itself, not the quoted context. Keep the quote if the whole body
    // is quoted (nothing else to show).
    let skip_quotes = body.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('>')
    });
    let mut rows = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if skip_quotes && trimmed.starts_with('>') {
            continue;
        }

        let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        rows.extend(
            crate::app::common::markdown::wrap_plain_line(
                &normalized,
                width.saturating_sub(2).max(1),
            )
            .into_iter()
            .map(|line| line.trim_end().to_string()),
        );
    }

    if rows.is_empty() {
        rows.push(String::new());
    }

    let truncated = rows.len() > max_rows;
    finalize_preview_rows(rows, max_rows, truncated, width)
}

fn finalize_preview_rows(
    mut rows: Vec<String>,
    max_rows: usize,
    truncated: bool,
    width: usize,
) -> Vec<String> {
    if rows.is_empty() {
        rows.push(String::new());
    }

    if rows.len() > max_rows {
        rows.truncate(max_rows);
    }

    let count = rows.len();
    for (index, row) in rows.iter_mut().enumerate() {
        let leading = usize::from(index == 0);
        let trailing = usize::from(index + 1 == count);
        let room = width.saturating_sub(leading + trailing);
        let content = if truncated && trailing > 0 {
            text_layout::truncate_with(&format!("{row}..."), room, "...")
        } else {
            text_layout::clip(row, room)
        };
        *row = text_layout::clip(
            &format!(
                "{}{content}{}",
                if leading > 0 { "\"" } else { "" },
                if trailing > 0 { "\"" } else { "" }
            ),
            width,
        );
    }

    rows
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
