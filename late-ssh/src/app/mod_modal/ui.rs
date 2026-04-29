use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;
use crate::app::input::{MouseEvent, MouseEventKind};

use super::state::{ModLogKind, ModLogLine, ModModalState};

const MODAL_WIDTH: u16 = 92;
const MODAL_HEIGHT: u16 = 28;

pub fn draw(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Moderation ")
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
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_log(frame, layout[0], state);
    draw_input(frame, layout[1], state);
    draw_footer(frame, layout[2]);
}

pub fn mouse_scroll_delta(mouse: MouseEvent) -> Option<i16> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(3),
        MouseEventKind::ScrollDown => Some(-3),
        _ => None,
    }
}

fn draw_log(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let height = area.height as usize;
    let log = state.log();
    let max_start = log.len().saturating_sub(height);
    let start = max_start
        .saturating_sub(state.scroll() as usize)
        .min(max_start);
    let lines: Vec<Line<'static>> = log.iter().skip(start).take(height).map(log_line).collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_input(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let block = Block::default()
        .title(" Command ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(state.command_input(), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" run  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Ctrl+L", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" clear  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn log_line(line: &ModLogLine) -> Line<'static> {
    let style = match line.kind {
        ModLogKind::Input => Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
        ModLogKind::Info => Style::default().fg(theme::TEXT_DIM()),
        ModLogKind::Success => Style::default().fg(theme::SUCCESS()),
        ModLogKind::Error => Style::default().fg(theme::ERROR()),
    };
    Line::from(Span::styled(line.text.clone(), style))
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
