//! The guide over the street: one framed box, the sections as a heading
//! and its lines, scrolled by the state. Pure.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use super::data::SECTIONS;
use super::state::State;
use crate::app::common::theme;

/// The box: as wide as the frame allows up to this, and as tall.
const MAX_WIDTH: u16 = 84;
const MAX_HEIGHT: u16 = 40;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let popup = centered_rect(area, MAX_WIDTH, MAX_HEIGHT);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(
            " the street, explained ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .padding(Padding::new(2, 2, 1, 0))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let [body, keys] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);

    let lines = body_lines();
    // The wrapped height is what the scroll holds against, so the measure
    // counts wrapped rows, not lines.
    let wrapped = Paragraph::new(lines.clone())
        .wrap(Wrap { trim: false })
        .line_count(body.width) as u16;
    state.record_page(wrapped, body.height);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll(), 0)),
        body,
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(vec![
                Span::styled("j/k", Style::default().fg(theme::AMBER())),
                Span::styled(" scroll   ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc", Style::default().fg(theme::ERROR())),
                Span::styled(" close   ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("?", Style::default().fg(theme::AMBER())),
                Span::styled(
                    " opens this again, any time on the street",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]),
        ]),
        keys,
    );
}

/// The copy as lines: a bright heading per section, its lines under it,
/// a blank between sections.
pub(crate) fn body_lines() -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let mut lines = Vec::new();
    for (i, section) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(section.title, heading)));
        for line in section.lines {
            lines.push(Line::from(Span::styled(*line, text)));
        }
    }
    lines
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
