use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use crate::app::common::theme;
use crate::app::input::{MouseEvent, MouseEventKind};

use super::state::{ModLogKind, ModLogLine, ModModalState};

pub fn draw(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let popup = centered_percent_rect(80, 80, area);
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    let height = inner.height as usize;
    let log = state.log();
    let start = state.viewport_start(height);
    let lines: Vec<Line<'static>> = log.iter().skip(start).take(height).map(log_line).collect();
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);

    if log.len() > height {
        let mut scrollbar_state = ScrollbarState::new(log.len())
            .position(start.min(log.len().saturating_sub(1)))
            .viewport_content_length(height);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(Style::default().fg(theme::BORDER()))
            .thumb_style(Style::default().fg(theme::AMBER_DIM()));
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
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
        Span::styled(" clear screen  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ PgUp/PgDn", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
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
        ModLogKind::Separator => Style::default().fg(theme::AMBER_DIM()),
        ModLogKind::Info => Style::default().fg(theme::TEXT_DIM()),
        ModLogKind::Success => Style::default().fg(theme::SUCCESS()),
        ModLogKind::Error => Style::default().fg(theme::ERROR()),
    };
    Line::from(Span::styled(line.text.clone(), style))
}

fn centered_percent_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let width = percent_of(area.width, width_percent).max(1);
    let height = percent_of(area.height, height_percent).max(1);
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn percent_of(value: u16, percent: u16) -> u16 {
    ((value as u32 * percent as u32) / 100) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn draw_log_keeps_latest_line_above_command_input() {
        let backend = TestBackend::new(100, 32);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut state = ModModalState::new();
        for idx in 0..40 {
            state.append_info(format!("line {idx:02}"));
        }

        terminal
            .draw(|frame| draw(frame, frame.area(), &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }

        assert!(
            text.contains("line 39"),
            "latest log line should render above the command box:\n{text}"
        );
    }
}
