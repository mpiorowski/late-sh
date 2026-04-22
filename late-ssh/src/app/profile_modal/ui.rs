use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{
    common::{markdown::render_body_to_lines, theme, time::timezone_current_time},
    settings_modal::data::country_label,
};

use super::state::ProfileModalState;

const MODAL_WIDTH: u16 = 92;
const MODAL_HEIGHT: u16 = 28;

pub fn draw(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" {} ", state.title()))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let content_area = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    let layout = Layout::vertical([Constraint::Min(8), Constraint::Length(1)]).split(content_area);

    let lines = build_lines(state, content_area.width as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset(), 0)),
        layout[0],
    );

    let footer = Line::from(vec![
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[1]);
}

fn build_lines(state: &ProfileModalState, width: usize) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());

    if state.loading() {
        return Vec::new();
    }

    let Some(profile) = state.profile() else {
        return Vec::new();
    };

    let username = if profile.username.trim().is_empty() {
        "not set"
    } else {
        profile.username.trim()
    };

    let mut lines = vec![
        Line::from(""),
        section_heading("Profile"),
        Line::from(vec![
            Span::styled("Username: ", dim),
            Span::styled(username.to_string(), text),
        ]),
        Line::from(vec![
            Span::styled("Country:  ", dim),
            Span::styled(country_label(profile.country.as_deref()), text),
        ]),
        Line::from(vec![
            Span::styled("Timezone: ", dim),
            Span::styled(
                profile.timezone.as_deref().unwrap_or("Not set").to_string(),
                text,
            ),
        ]),
    ];

    if let Some(current_time) = timezone_current_time(Utc::now(), profile.timezone.as_deref()) {
        lines.push(Line::from(vec![
            Span::styled("Current time: ", dim),
            Span::styled(current_time, text),
        ]));
    }

    lines.extend([Line::from(""), section_heading("Bio")]);

    if profile.bio.trim().is_empty() {
        lines.push(Line::from(Span::styled("Not set", dim)));
    } else {
        lines.extend(render_body_to_lines(
            &profile.bio,
            width,
            Span::raw(""),
            text,
        ));
    }

    lines
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
