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

    frame.render_widget(Paragraph::new(section_heading("Shop")), sections[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Spend Late Chips on cosmetics, boosters and prestige items. Coming in v2.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let categories = vec![
        bullet("Chat presence", "username colors · titles · custom join lines"),
        bullet("Profile", "frames · banners · portraits · bio styling"),
        bullet("Bonsai & aquarium", "tree species · pots · weather · fish"),
        bullet("Game cosmetics", "card backs · felt · piece themes · tile skins"),
        bullet("Music boosters", "force-vote · skip-vote · queue-jump (consumable)"),
        bullet("Themes & dashboard", "premium themes · MOTD lines · palette tweaks"),
        bullet("Seasonal drops", "monthly limited cosmetics · holiday badges"),
    ];
    frame.render_widget(Paragraph::new(categories), sections[3]);
}

fn bullet(label: &str, hint: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("   · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            format!("{label:<20}"),
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
