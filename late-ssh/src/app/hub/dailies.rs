use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

pub fn draw(frame: &mut Frame, area: Rect) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing
        Constraint::Min(0),    // body
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Dailies")), sections[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Your daily-puzzle dashboard. Coming in v2.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let upcoming = vec![
        bullet("Today's puzzles", "sudoku · nonogram · solitaire · minesweeper status"),
        bullet("Chips earned today", "50 easy · 100 medium · 150 hard, per puzzle"),
        bullet("Win streaks", "running streak per game, longest streak this month"),
        bullet("Daily reset", "countdown to the next UTC rollover"),
    ];
    frame.render_widget(Paragraph::new(upcoming), sections[3]);
}

fn bullet(label: &str, hint: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "   · ",
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            format!("{label:<22}"),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled(
            hint.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}
