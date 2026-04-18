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
const PREVIEW_ROWS: usize = 2;

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
        let preview_rows = preview_rows(&item.message_preview, preview_width, PREVIEW_ROWS);
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

fn preview_rows(body: &str, width: usize, max_rows: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for word in trimmed.split_whitespace() {
            let next_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };

            if next_len > width && !current.is_empty() {
                rows.push(current);
                current = word.to_string();
            } else if current.is_empty() {
                current.push_str(word);
            } else {
                current.push(' ');
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            rows.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        rows.push(current);
    }

    if rows.is_empty() {
        rows.push(String::new());
    }

    let truncated = rows.len() > max_rows;
    finalize_preview_rows(rows, max_rows, truncated)
}

fn finalize_preview_rows(mut rows: Vec<String>, max_rows: usize, truncated: bool) -> Vec<String> {
    if rows.is_empty() {
        rows.push(String::new());
    }

    if rows.len() > max_rows {
        rows.truncate(max_rows);
    }

    if truncated && let Some(last) = rows.last_mut() {
        last.push_str("...");
    }

    if let Some(first) = rows.first_mut() {
        first.insert(0, '"');
    }
    if let Some(last) = rows.last_mut() {
        last.push('"');
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::preview_rows;

    #[test]
    fn preview_rows_wraps_into_two_rows() {
        let rows = preview_rows(
            "@mat this is a long mention preview that should use both rows in the mentions panel",
            24,
            2,
        );

        assert_eq!(rows.len(), 2);
        assert!(rows[0].starts_with('"'));
        assert!(rows[1].ends_with('"'));
    }

    #[test]
    fn preview_rows_uses_multiple_source_lines() {
        let rows = preview_rows("> quoted line\nactual reply line", 40, 2);

        assert_eq!(
            rows,
            vec![
                "\"> quoted line".to_string(),
                "actual reply line\"".to_string()
            ]
        );
    }

    #[test]
    fn preview_rows_handles_empty_preview() {
        let rows = preview_rows("", 20, 2);

        assert_eq!(rows, vec!["\"\"".to_string()]);
    }
}
