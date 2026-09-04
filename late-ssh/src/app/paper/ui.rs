//! The Late Edition's modal: a centered box over whatever screen is up,
//! same shape as the login announcements.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{PaperInk, PaperLine, PaperModal};
use crate::app::common::theme;

/// The paper takes most of the screen: on a busy day it is many rooms of
/// five lines each, and a small box would be all scrolling.
const PAPER_WIDTH_PERMILLE: u16 = 900;
const PAPER_HEIGHT_PERMILLE: u16 = 900;
const PAPER_MAX_WIDTH: u16 = 160;

pub(crate) fn draw(frame: &mut Frame, area: Rect, modal: &PaperModal) {
    let width = permille(area.width, PAPER_WIDTH_PERMILLE).clamp(24, PAPER_MAX_WIDTH);
    let height = permille(area.height, PAPER_HEIGHT_PERMILLE).max(5);
    let popup = centered_rect(width, height, area);
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
        Paragraph::new(modal.lines.iter().map(ink_line).collect::<Vec<_>>())
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

/// The palette, read here and nowhere earlier: `theme`'s thread local
/// holds the theme of whichever session is being drawn on this thread, and
/// it is set at the top of `App::render`. A line styled back when the tick
/// laid the paper out would take a stranger's colours.
fn ink_style(ink: PaperInk) -> Style {
    match ink {
        PaperInk::Heading => Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
        PaperInk::Title => Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
        PaperInk::Meta => Style::default().fg(theme::TEXT_DIM()),
        PaperInk::Bumped => Style::default().fg(theme::AMBER_GLOW()),
        PaperInk::JoinHint => Style::default().fg(theme::AMBER_DIM()),
        PaperInk::Body => Style::default().fg(theme::TEXT()),
        PaperInk::Faint => Style::default().fg(theme::TEXT_FAINT()),
    }
}

fn ink_line(line: &PaperLine) -> Line<'static> {
    Line::from(
        line.iter()
            .map(|span| Span::styled(span.text.clone(), ink_style(span.ink)))
            .collect::<Vec<_>>(),
    )
}

/// `value * permille / 1000` without the `u16` overflow a wide terminal
/// would hit.
fn permille(value: u16, permille: u16) -> u16 {
    (u32::from(value) * u32::from(permille) / 1000) as u16
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

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
