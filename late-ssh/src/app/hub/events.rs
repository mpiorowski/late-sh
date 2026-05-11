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

    frame.render_widget(Paragraph::new(section_heading("Events")), sections[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Seasonal drops, live tournaments and community moments. Coming in v2.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let upcoming = vec![
        bullet("Monthly reset", "top 3 of each board freeze into permanent profile badges"),
        bullet("Seasonal drops", "halloween · christmas · new year, monthly themed cosmetics"),
        bullet("Tournaments", "live poker tables · arcade speedruns · scored events"),
        bullet("Anniversary", "yearly returning cosmetics for long-time members"),
    ];
    frame.render_widget(Paragraph::new(upcoming), sections[3]);
}

fn bullet(label: &str, hint: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("   · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            format!("{label:<18}"),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled(hint.to_string(), Style::default().fg(theme::TEXT_DIM())),
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
