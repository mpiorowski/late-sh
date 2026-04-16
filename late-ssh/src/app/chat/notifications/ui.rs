use crate::app::common::{primitives::format_relative_time, theme};
use late_core::models::notification::NotificationView;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct NotificationListView<'a> {
    pub items: &'a [NotificationView],
    pub selected_index: usize,
}

const ITEM_HEIGHT: u16 = 4;

pub fn draw_notification_list(frame: &mut Frame, area: Rect, view: &NotificationListView<'_>) {
    let selected = if view.items.is_empty() {
        0
    } else {
        view.selected_index.min(view.items.len() - 1) + 1
    };
    let title = format!(" Mentions ({selected}/{}) ", view.items.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme::BORDER()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

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

        let bg_color = if idx == selected_index {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };

        let item_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(Style::default().bg(bg_color));

        let content_area = item_block.inner(item_area);
        frame.render_widget(item_block, item_area);

        let room_label = item
            .room_slug
            .as_deref()
            .map(|s| format!("#{s}"))
            .unwrap_or_else(|| "DM".to_string());

        let read_indicator = if item.read_at.is_some() {
            Span::styled(" ", Style::default())
        } else {
            Span::styled("* ", Style::default().fg(theme::MENTION()))
        };

        let lines = vec![
            Line::from(vec![
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
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    preview_text(&item.message_preview),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]),
        ];

        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(p, content_area);
    }
}

fn preview_text(body: &str) -> String {
    let first_line = body.lines().next().unwrap_or("");
    let trimmed = first_line.trim();
    if trimmed.chars().count() > 80 {
        format!("\"{}...\"", trimmed.chars().take(77).collect::<String>())
    } else {
        format!("\"{trimmed}\"")
    }
}
