use late_core::models::character_sheet::SHEET_BODY_MAX_CHARS;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

use super::state::{SheetField, SheetModalState};

const MODAL_WIDTH: u16 = 80;
const MODAL_HEIGHT: u16 = 28;

pub fn draw(frame: &mut Frame, area: Rect, state: &SheetModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" character sheet · {} ", state.target_username()))
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
        Constraint::Length(1), // name row
        Constraint::Length(1), // body header
        Constraint::Min(4),    // body
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_name_row(frame, layout[1], state);
    draw_body_header(frame, layout[2], state);
    frame.render_widget(state.body_input(), layout[3].inner(Margin::new(2, 0)));
    draw_footer(frame, layout[4], state);
}

fn draw_name_row(frame: &mut Frame, area: Rect, state: &SheetModalState) {
    let columns = Layout::horizontal([Constraint::Length(8), Constraint::Min(8)])
        .split(area.inner(Margin::new(2, 0)));
    let focused = state.focus() == SheetField::Name;
    frame.render_widget(
        Paragraph::new(Span::styled("Name", label_style(state, focused))),
        columns[0],
    );

    if state.editing() && focused {
        frame.render_widget(state.name_input(), columns[1]);
        return;
    }
    let name = state.name_text();
    let value = if name.is_empty() {
        Span::styled("unnamed", Style::default().fg(theme::TEXT_FAINT()))
    } else {
        Span::styled(name, Style::default().fg(theme::TEXT_BRIGHT()))
    };
    frame.render_widget(Paragraph::new(value), columns[1]);
}

fn draw_body_header(frame: &mut Frame, area: Rect, state: &SheetModalState) {
    let focused = state.focus() == SheetField::Body;
    let mut spans = vec![Span::styled("Sheet", label_style(state, focused))];
    if state.editable() {
        spans.push(Span::styled(
            format!(
                "   {}/{} chars",
                state.body_char_count(),
                SHEET_BODY_MAX_CHARS
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        area.inner(Margin::new(2, 0)),
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &SheetModalState) {
    let hint = if !state.editable() {
        "read-only · j/k scroll · esc close"
    } else if state.editing() {
        match state.focus() {
            SheetField::Name => "↵ save · esc cancel",
            SheetField::Body => "↵ save · alt+↵ newline",
        }
    } else {
        "tab switch field · ↵ edit · esc close"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("  {hint}"),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        area,
    );
}

/// Amber-bold when the row is focused (and not mid-edit), dim otherwise.
fn label_style(state: &SheetModalState, focused: bool) -> Style {
    if focused && !state.editing() {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    }
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
