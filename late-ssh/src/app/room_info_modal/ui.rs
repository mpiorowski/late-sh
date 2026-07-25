use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{Field, Mode, RoomInfoModalState};
use crate::app::common::theme;

/// Draw the room-info form centred over `area`. Follows the poll modal's field
/// convention: one bordered box per field, label and character count in its
/// title, focus carried by the border.
pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &RoomInfoModalState) {
    let popup = centered_rect(area, 64, 12);
    frame.render_widget(Clear, popup);

    let creating = matches!(state.mode(), Some(Mode::Create { .. }));
    let heading = if creating {
        format!(" Create {} ", state.room_label())
    } else {
        format!(" {} ", state.room_label())
    };
    let block = Block::default()
        .title(Span::styled(
            heading,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // owner + keys
        Constraint::Length(3), // topic
        Constraint::Length(3), // rules
    ])
    .split(inner);

    let submit = if creating {
        "Enter create"
    } else {
        "Enter save"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Owner ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                state.owner_label().to_string(),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled("  ·  ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(submit, Style::default().fg(theme::SUCCESS())),
            Span::styled("  Tab field  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::ERROR())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]))
        .style(Style::default().bg(theme::BG_CANVAS())),
        rows[0],
    );

    draw_field(frame, rows[1], state, Field::Topic, "What it is about");
    draw_field(frame, rows[2], state, Field::Rules, "Rules");
}

fn draw_field(
    frame: &mut Frame,
    area: Rect,
    state: &RoomInfoModalState,
    field: Field,
    label: &str,
) {
    let focused = state.focus() == field;
    let input = state.field(field);
    let border = if focused {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let title = format!(
        " {label} {}/{} ",
        input.lines().join(" ").chars().count(),
        field.max_len()
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused {
                    theme::TEXT_BRIGHT()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(input, inner);
}

/// A centred rectangle of the given size, clamped to `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
