use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::{data::HelpTopic, state::HelpModalState};

pub fn draw(frame: &mut Frame, area: Rect, state: &HelpModalState) {
    let popup = centered_rect(96, 34, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Guide ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // tabs
        Constraint::Length(1), // breathing room
        Constraint::Min(8),    // body
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_tabs(frame, layout[1], state.selected_topic());

    let body = layout[3].inner(Margin::new(2, 0));
    let lines: Vec<Line> = state
        .current_lines()
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::TEXT()))))
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.current_scroll(), 0)),
        body,
    );

    let footer = Line::from(vec![
        Span::styled("  ←/→ h/l", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" switch slides  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[4]);
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: HelpTopic) {
    let mut spans = vec![Span::raw("  ")];
    for topic in HelpTopic::ALL {
        let active = topic == selected;
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", topic.short_label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
