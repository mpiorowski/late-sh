use chrono::Utc;
use late_core::models::bonsai::Tree;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{
    bonsai::{state::stage_for, ui::render_tree_art_lines},
    chat::showcase::svc::ShowcaseFeedItem,
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
    let body_area = layout[0];
    let use_side_boxes = body_area.width >= 74;
    let body_columns = if use_side_boxes {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(42), Constraint::Length(30)])
            .split(body_area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(body_area)
    };

    let lines = build_lines(state, body_columns[0].width as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset(), 0)),
        body_columns[0],
    );

    if use_side_boxes {
        draw_side_boxes(frame, body_columns[1], state);
    }

    let footer = Line::from(vec![
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[1]);
}

fn draw_side_boxes(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let boxes = Layout::vertical([Constraint::Length(12), Constraint::Min(8)]).split(area);
    draw_bonsa_box(frame, boxes[0], state.bonsai());
    draw_late_fetch_box(frame, boxes[1], state);
}

fn draw_bonsa_box(frame: &mut Frame, area: Rect, tree: Option<&Tree>) {
    let block = Block::default()
        .title(" bonsa ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(tree) = tree else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " no bonsai yet",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            inner,
        );
        return;
    };

    let stage = stage_for(tree.is_alive, tree.growth_points);
    let age_days = (Utc::now().date_naive() - tree.created.date_naive())
        .num_days()
        .max(0);
    let wilting = tree.is_alive
        && tree
            .last_watered
            .map(|last| (Utc::now().date_naive() - last).num_days() >= 2)
            .unwrap_or(age_days >= 2);

    let mut lines =
        render_tree_art_lines(stage, tree.seed, wilting, inner.width as usize, 0.0, None);
    lines.push(
        Line::from(vec![Span::styled(
            format!("{} · {}d", stage.label(), age_days),
            Style::default().fg(theme::TEXT_DIM()),
        )])
        .centered(),
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_late_fetch_box(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let block = Block::default()
        .title(" late.fetch ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(profile) = state.profile() else {
        return;
    };

    let theme_id = profile.theme_id.as_deref().unwrap_or("late");
    let rows = [
        (
            "created",
            profile
                .created_at
                .as_ref()
                .map(format_created_at)
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        ("theme", theme::label_for_id(theme_id).to_string()),
        (
            "ide",
            profile.ide.clone().unwrap_or_else(|| "not set".to_string()),
        ),
        (
            "terminal",
            profile
                .terminal
                .clone()
                .unwrap_or_else(|| "not set".to_string()),
        ),
        (
            "os",
            profile.os.clone().unwrap_or_else(|| "not set".to_string()),
        ),
        ("showcases", state.showcase_count_for_viewed().to_string()),
    ];

    let lines: Vec<Line<'static>> = rows
        .into_iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(
                    format!("{label:<9} "),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(value, Style::default().fg(theme::TEXT())),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn format_created_at(created_at: &chrono::DateTime<Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
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

    let showcases = state.showcases_for_viewed();
    if !showcases.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_heading(&format!("Showcases ({})", showcases.len())));
        for item in showcases {
            lines.push(Line::from(""));
            lines.extend(render_body_to_lines(
                &showcase_markdown(item),
                width,
                Span::raw(""),
                text,
            ));
        }
    }

    lines
}

fn showcase_markdown(item: &ShowcaseFeedItem) -> String {
    let s = &item.showcase;
    let mut out = String::new();
    out.push_str("### ");
    out.push_str(s.title.trim());
    out.push_str("\n\n> ");
    out.push_str(s.url.trim());
    let description = s.description.trim();
    if !description.is_empty() {
        out.push_str("\n\n");
        out.push_str(description);
    }
    if !s.tags.is_empty() {
        out.push_str("\n\n");
        let mut first = true;
        for tag in &s.tags {
            if !first {
                out.push(' ');
            }
            first = false;
            out.push('`');
            out.push('#');
            out.push_str(tag);
            out.push('`');
        }
    }
    out
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
