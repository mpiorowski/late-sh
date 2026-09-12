//! The `/status` picker: one screen, no wizard. Eight numbered status rows,
//! one armed duration, and a live sentence saying what the pick will actually
//! do. That sentence is the point of the modal: the clearing rule is the one
//! thing about statuses nobody can infer from the badge.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    common::status::Status,
    common::theme,
    status_picker::state::{DURATIONS, StatusPickerState},
};

const MODAL_WIDTH: u16 = 40;
/// Border, a blank, eight rows, a blank, the duration row, a blank, the hint,
/// and the footer.
const MODAL_HEIGHT: u16 = 17;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &StatusPickerState) {
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width.saturating_sub(4)).max(20),
        MODAL_HEIGHT.min(area.height.saturating_sub(2)).max(5),
    );
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Status ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 5 || inner.width < 20 {
        frame.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_statuses(frame, layout[0], state);
    draw_duration(frame, layout[2], state);
    draw_hint(frame, layout[4], state);
    draw_footer(frame, layout[5]);
}

fn draw_statuses(frame: &mut Frame, area: Rect, state: &StatusPickerState) {
    let rows: Vec<Line> = Status::ALL
        .into_iter()
        .enumerate()
        .map(|(index, status)| {
            let selected = index == state.selected_index();
            let (marker, style) = if selected {
                (
                    "▸",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (" ", Style::default().fg(theme::TEXT()))
            };
            Line::from(vec![
                Span::styled(
                    format!(" {marker} {}  ", index + 1),
                    Style::default().fg(theme::TEXT_MUTED()),
                ),
                Span::styled(format!("{}  {}", status.glyph(), status.word()), style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

fn draw_duration(frame: &mut Frame, area: Rect, state: &StatusPickerState) {
    let label = match state.selected_minutes() {
        Some(minutes) => format!("{minutes}m"),
        None => "no timer".to_string(),
    };
    // The dots show how many stops the cycle has, so the arrows read as a
    // ring rather than a field that might take typing.
    let dots: String = (0..DURATIONS.len())
        .map(|index| {
            if index == state.duration_index() {
                '•'
            } else {
                '·'
            }
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("   for  ", Style::default().fg(theme::TEXT_MUTED())),
            Span::styled("‹ ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(
                format!("{label:^9}"),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ›  ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(dots, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        area,
    );
}

/// What the current pick will do, in the same words the command banner uses.
fn draw_hint(frame: &mut Frame, area: Rect, state: &StatusPickerState) {
    let hint = match state.selected_minutes() {
        Some(minutes) => format!("clears in {minutes}m, stays while you chat"),
        None => "clears when you next post".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("   {hint}"),
            Style::default().fg(theme::TEXT_MUTED()),
        ))),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "   ↑↓ status   ←→ timer   ⏎ set   esc cancel",
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        area,
    );
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
