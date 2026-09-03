//! The Late Edition's modal: a centered box over whatever screen is up,
//! same shape as the login announcements.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::PaperModal;
use crate::app::common::theme;

pub(crate) fn draw(frame: &mut Frame, area: Rect, modal: &PaperModal) {
    let popup = centered_rect(84, 34, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(modal.title.clone())
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.width < 24 || inner.height < 5 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let body_area = layout[1].inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    frame.render_widget(
        Paragraph::new(modal.lines.clone())
            .wrap(Wrap { trim: false })
            .scroll((modal.scroll_offset, 0)),
        body_area,
    );

    let footer = Line::from(vec![
        Span::styled(" j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("/paper", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            " reopens it any time",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(footer).centered(), layout[2]);
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
