use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::{data::HelpTopic, state::HelpModalState};

pub const MODAL_WIDTH: u16 = 96;
pub const MODAL_HEIGHT: u16 = 34;

pub fn draw(frame: &mut Frame, area: Rect, state: &HelpModalState) {
<<<<<<< HEAD
    let popup = centered_percent_rect(80, 85, area);
=======
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
>>>>>>> d2fc511 (update)
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

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // tabs
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // body
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

    draw_footer(frame, layout[4]);
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: HelpTopic) {
    let mut spans = vec![Span::raw("  ")];
    for topic in HelpTopic::ALL {
        let active = topic == selected;
        let active_style = Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD);
        let style = if active {
            active_style
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", topic.short_label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
<<<<<<< HEAD
        Span::styled("?/Esc/q", Style::default().fg(theme::AMBER_DIM())),
=======
        Span::styled("Ctrl+P/Esc/q", Style::default().fg(theme::AMBER_DIM())),
>>>>>>> d2fc511 (update)
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}

<<<<<<< HEAD
fn centered_percent_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
=======
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
>>>>>>> d2fc511 (update)
}
