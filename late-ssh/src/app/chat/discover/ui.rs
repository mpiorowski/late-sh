use crate::app::chat::svc::DiscoverRoomItem;
use crate::app::common::{primitives::format_relative_time, theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct DiscoverListView<'a> {
    pub items: &'a [DiscoverRoomItem],
    pub selected_index: usize,
    pub loading: bool,
}

const ITEM_HEIGHT: u16 = 5;

pub fn draw_discover_list(frame: &mut Frame, area: Rect, view: &DiscoverListView<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Discover ")
        .border_style(Style::default().fg(theme::BORDER()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if view.loading {
        let text = Text::from("Loading rooms...");
        let loading_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(loading_p, inner_area);
        return;
    }

    if view.items.is_empty() {
        let text = Text::from("No public rooms to discover right now.");
        let empty_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(empty_p, inner_area);
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

        let activity = item
            .last_message_at
            .map(format_relative_time)
            .unwrap_or_else(|| "no messages yet".to_string());
        let member_noun = if item.member_count == 1 {
            "member"
        } else {
            "members"
        };
        let message_noun = if item.message_count == 1 {
            "message"
        } else {
            "messages"
        };

        let lines = vec![
            Line::from(vec![Span::styled(
                format!("#{}", item.slug),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled(
                    format!("{} {}", item.member_count, member_noun),
                    Style::default().fg(theme::AMBER()),
                ),
                Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    format!("{} {}", item.message_count, message_noun),
                    Style::default().fg(theme::TEXT()),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!("Last activity: {activity}"),
                Style::default().fg(theme::TEXT_DIM()),
            )]),
        ];

        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, content_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_discover(view: DiscoverListView<'_>) -> String {
        let width = 60;
        let height = 10;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| draw_discover_list(frame, Rect::new(0, 0, width, height), &view))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..height {
            for x in 0..width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        rendered
    }

    #[test]
    fn loading_state_does_not_claim_there_are_no_rooms() {
        let rendered = render_discover(DiscoverListView {
            items: &[],
            selected_index: 0,
            loading: true,
        });

        assert!(rendered.contains("Loading rooms..."));
        assert!(!rendered.contains("No public rooms"));
    }

    #[test]
    fn loaded_empty_state_explains_no_discoverable_rooms() {
        let rendered = render_discover(DiscoverListView {
            items: &[],
            selected_index: 0,
            loading: false,
        });

        assert!(rendered.contains("No public rooms to discover right now."));
    }
}
