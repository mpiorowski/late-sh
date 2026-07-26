use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::common::theme;

use super::state::ScratchpadState;

/// Minimal keyboard-only surface for v1: a one-line "paired with @user"
/// header plus the shared `TextArea` filling the rest. No line numbers, no
/// syntax highlighting, no mouse click targets (see module `CONTEXT.md`).
pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &ScratchpadState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let header = if state.partner_left() {
        Line::from(Span::styled(
            format!("@{} left the pairing. Esc to exit", state.partner_username),
            Style::default().fg(theme::TEXT_DIM()),
        ))
    } else {
        Line::from(vec![
            Span::styled("paired with ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("@{}", state.partner_username),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                format!(
                    "  (line {})  Esc to leave",
                    state.partner_cursor.0.saturating_add(1)
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(header), chunks[0]);
    frame.render_widget(&state.editor, chunks[1]);
}
