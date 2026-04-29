use chrono::Utc;
use late_core::models::bonsai::Tree;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
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
// Match the right-sidebar bonsai card width (see common/sidebar.rs).
const BONSAI_CARD_WIDTH: u16 = 24;
const FETCH_STRIP_HEIGHT: u16 = 4;

pub fn draw(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::vertical([
        Constraint::Min(10),
        Constraint::Length(FETCH_STRIP_HEIGHT),
        Constraint::Length(1),
    ])
    .split(popup);

    let wide = layout[0].width >= 80;
    if wide {
        let body = Layout::horizontal([Constraint::Min(50), Constraint::Length(BONSAI_CARD_WIDTH)])
            .split(layout[0]);
        draw_profile_card(frame, body[0], state);
        draw_bonsai_card(frame, body[1], state.bonsai());
    } else {
        draw_profile_card(frame, layout[0], state);
    }

    draw_late_fetch_strip(frame, layout[1], state);
    draw_footer(frame, layout[2]);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}

fn draw_profile_card(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let block = Block::default()
        .title(" profile ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    let lines = build_profile_lines(state, content.width as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset(), 0)),
        content,
    );
}

fn draw_bonsai_card(frame: &mut Frame, area: Rect, tree: Option<&Tree>) {
    let block = Block::default()
        .title(" bonsai ")
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

    let visible = inner.height as usize;
    let label_line = Line::from(vec![Span::styled(
        format!("{} · {}d", stage.label(), age_days),
        Style::default().fg(theme::TEXT_DIM()),
    )])
    .centered();

    if lines.len() + 1 < visible {
        let pad = visible.saturating_sub(lines.len() + 1);
        for _ in 0..pad {
            lines.insert(0, Line::from(""));
        }
    }
    lines.push(label_line);

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_late_fetch_strip(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let block = Block::default()
        .title(" late.fetch ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    frame.render_widget(block, area);

    let Some(profile) = state.profile() else {
        return;
    };

    let dim = Style::default().fg(theme::TEXT_DIM());
    let label = Style::default().fg(theme::AMBER_DIM());
    let value = Style::default().fg(theme::TEXT());

    let theme_id = profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID);
    let created = profile
        .created_at
        .as_ref()
        .map(format_created_at)
        .unwrap_or_else(|| "unknown".to_string());
    let ide = profile.ide.clone().unwrap_or_else(|| "—".to_string());
    let terminal = profile.terminal.clone().unwrap_or_else(|| "—".to_string());
    let os = profile.os.clone().unwrap_or_else(|| "—".to_string());
    let theme_label = theme::label_for_id(theme_id).to_string();
    let langs = if profile.langs.is_empty() {
        "—".to_string()
    } else {
        profile.langs.join(", ")
    };

    let inner_w = inner.width as usize;
    let col_w = inner_w / 3;

    let row1 = Line::from(format_three_cells(
        ("created", &created),
        ("ide", &ide),
        ("os", &os),
        col_w,
        label,
        value,
        dim,
    ));
    let row2 = Line::from(format_three_cells(
        ("theme", &theme_label),
        ("terminal", &terminal),
        ("langs", &langs),
        col_w,
        label,
        value,
        dim,
    ));

    frame.render_widget(Paragraph::new(vec![row1, row2]), inner);
}

#[allow(clippy::too_many_arguments)]
fn format_three_cells(
    a: (&str, &str),
    b: (&str, &str),
    c: (&str, &str),
    col_w: usize,
    label_style: Style,
    value_style: Style,
    sep_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (label, value)) in [a, b, c].into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("│ ", sep_style));
        }
        let label_padded = format!("{label:<9} ");
        let used = label_padded.chars().count() + value.chars().count();
        let pad = col_w.saturating_sub(used + if i < 2 { 2 } else { 0 });
        spans.push(Span::styled(label_padded, label_style));
        spans.push(Span::styled(value.to_string(), value_style));
        if i < 2 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
    }
    spans
}

fn format_created_at(created_at: &chrono::DateTime<Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
}

fn build_profile_lines(state: &ProfileModalState, width: usize) -> Vec<Line<'static>> {
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
