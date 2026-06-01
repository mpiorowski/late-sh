use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{
    bonsai::{state::stage_for, ui::render_tree_art_lines},
    bonsai_v2::render::render_tree_lines,
    chat::showcase::svc::ShowcaseFeedItem,
    common::{markdown::render_body_to_lines, theme, time::timezone_current_time},
    hub::aquarium::{state::AquariumState, ui as aquarium_ui},
    settings_modal::data::country_label,
};

use super::{
    badges,
    state::{ProfileModalState, ProfileTab},
};

// Match the Settings modal so the two read as the same kind of panel.
const MODAL_WIDTH: u16 = 96;
const MODAL_HEIGHT: u16 = 34;
/// Pinned late.fetch card: 2 border rows + 3 grid rows.
const LATE_FETCH_BOX_HEIGHT: u16 = 5;

pub fn draw(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);

    // Two stacked boxes with a blank row between them: the profile box (glance
    // stats, tabs, and the active tab body) on top, and the always-visible
    // late.fetch card below it. Key hints live on a free line under both.
    let regions = Layout::vertical([
        Constraint::Min(8),                        // profile box
        Constraint::Length(1),                     // breathing gap between boxes
        Constraint::Length(LATE_FETCH_BOX_HEIGHT), // late.fetch box
        Constraint::Length(1),                     // footer hints
    ])
    .split(popup);

    // Clear only the boxes and the hint line, never the gap row (regions[1]),
    // so whatever is behind the modal shows through between the two boxes.
    frame.render_widget(Clear, regions[0]);
    frame.render_widget(Clear, regions[2]);
    frame.render_widget(Clear, regions[3]);

    draw_profile_box(frame, regions[0], state);
    draw_late_fetch_box(frame, regions[2], state);
    draw_footer(frame, regions[3], state);
}

fn draw_profile_box(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let block = Block::default()
        .title(format!(" profile · {} ", header_name(state)))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 6 || inner.width < 24 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room below the title border
        Constraint::Length(1), // identity glance
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // tabs
        Constraint::Length(1), // breathing room
        Constraint::Min(3),    // body
    ])
    .split(inner);

    draw_header(frame, layout[1], state);
    draw_tabs(frame, layout[3], state);

    let body = layout[5].inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    match state.tab() {
        ProfileTab::Overview => draw_overview(frame, body, state),
        ProfileTab::Bonsai => draw_bonsai_tab(frame, body, state),
        ProfileTab::Aquarium => draw_aquarium_tab(frame, body, state),
        ProfileTab::Badges => badges::draw(frame, body, state.badges(), state.scroll_offset()),
    }
}

/// The late.fetch card: its own framed box, holding only the neofetch-style
/// system grid. Kept visible under every tab so it reads as a fixed identity
/// footer rather than something you scroll to.
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

    if inner.height == 0 || inner.width < 12 {
        return;
    }

    let body = inner.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });

    let Some(profile) = state.profile() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "loading…",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            body,
        );
        return;
    };

    let lines = late_fetch_lines(profile, body.width as usize);
    frame.render_widget(Paragraph::new(lines), body);
}

fn header_name(state: &ProfileModalState) -> String {
    if let Some(profile) = state.profile() {
        let username = profile.username.trim();
        if !username.is_empty() {
            return username.to_string();
        }
    }
    if state.fallback_name().is_empty() {
        "loading".to_string()
    } else {
        state.fallback_name().to_string()
    }
}

fn draw_header(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let value = Style::default().fg(theme::TEXT_BRIGHT());
    let dim = Style::default().fg(theme::TEXT_DIM());

    let Some(profile) = state.profile() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("  loading…", dim))),
            area,
        );
        return;
    };

    let mut spans = vec![
        Span::raw("  "),
        Span::styled(country_label(profile.country.as_deref()), value),
    ];
    if let Some(time) = timezone_current_time(Utc::now(), profile.timezone.as_deref()) {
        spans.push(sep());
        spans.push(Span::styled(format!("{time} local"), value));
    }
    if let Some(balance) = state.chip_balance() {
        spans.push(sep());
        spans.push(Span::styled(format!("{balance} chips"), value));
    }
    if let Some(birthday) = profile.birthday.as_deref() {
        spans.push(sep());
        spans.push(Span::styled(format_birthday(birthday), value));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let selected = state.tab();
    let active = Style::default()
        .fg(theme::AMBER_GLOW())
        .bg(theme::BG_HIGHLIGHT())
        .add_modifier(Modifier::BOLD);
    let idle = Style::default().fg(theme::TEXT_DIM());

    let mut spans = vec![Span::raw("  ")];
    for tab in ProfileTab::ALL {
        let style = if tab == selected { active } else { idle };
        spans.push(Span::styled(format!(" {} ", tab.title()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut spans = vec![
        Span::raw("  "),
        Span::styled("Tab/S+Tab", key),
        Span::styled(" switch tabs  ", dim),
    ];
    if matches!(state.tab(), ProfileTab::Overview | ProfileTab::Badges) {
        spans.push(Span::styled("↑↓ j/k", key));
        spans.push(Span::styled(" scroll  ", dim));
    }
    spans.push(Span::styled("b", key));
    spans.push(Span::styled(" badges  ", dim));
    spans.push(Span::styled("Esc/q", key));
    spans.push(Span::styled(" close", dim));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_overview(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    if state.loading() {
        render_centered_dim(frame, area, "loading…");
        return;
    }
    if let Some(profile) = state.profile() {
        if profile.bio.trim().is_empty() && state.showcases_for_viewed().is_empty() {
            render_centered_dim(frame, area, "no bio or showcases yet");
            return;
        }
    }
    let lines = build_overview_lines(state, area.width as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset(), 0)),
        area,
    );
}

fn build_overview_lines(state: &ProfileModalState, width: usize) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());

    let Some(profile) = state.profile() else {
        return Vec::new();
    };

    let mut lines = vec![section_heading("Bio")];
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

fn late_fetch_lines(
    profile: &late_core::models::profile::Profile,
    width: usize,
) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let label = Style::default().fg(theme::AMBER_DIM());
    let value = Style::default().fg(theme::TEXT());

    let theme_id = profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID);
    let created = profile
        .created_at
        .as_ref()
        .map(format_created_at)
        .unwrap_or_else(|| "unknown".to_string());
    let ide = profile.ide.clone().unwrap_or_else(|| "not set".to_string());
    let terminal = profile
        .terminal
        .clone()
        .unwrap_or_else(|| "not set".to_string());
    let os = profile.os.clone().unwrap_or_else(|| "not set".to_string());
    let theme_label = theme::label_for_id(theme_id).to_string();
    let langs = if profile.langs.is_empty() {
        "not set".to_string()
    } else {
        profile.langs.join(", ")
    };

    let col_w = (width / 2).max(12);
    vec![
        Line::from(format_two_cells(
            ("created", &created),
            ("theme", &theme_label),
            col_w,
            label,
            value,
            dim,
        )),
        Line::from(format_two_cells(
            ("ide", &ide),
            ("terminal", &terminal),
            col_w,
            label,
            value,
            dim,
        )),
        Line::from(format_two_cells(
            ("os", &os),
            ("langs", &langs),
            col_w,
            label,
            value,
            dim,
        )),
    ]
}

fn draw_bonsai_tab(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    let tree_area = rows[0];
    let caption_area = rows[1];

    if state.dynamic_bonsai_selected() {
        if let Some(bonsai) = state.bonsai_v2() {
            let lines = render_tree_lines(
                bonsai,
                tree_area.width as usize,
                tree_area.height as usize,
                false,
            );
            bottom_align(frame, tree_area, lines);
            render_caption(
                frame,
                caption_area,
                &format!(
                    "Dynamic Bonsai · Day {} · vigor {} · stress {}",
                    bonsai.age_days, bonsai.vigor, bonsai.water_stress
                ),
                bonsai.is_alive,
            );
            return;
        }
        render_centered_dim(frame, area, "Dynamic Bonsai not planted yet");
        return;
    }

    if let Some(tree) = state.bonsai() {
        let stage = stage_for(tree.is_alive, tree.growth_points);
        let age_days = (Utc::now().date_naive() - tree.created.date_naive())
            .num_days()
            .max(0);
        let wilting = tree.is_alive
            && tree
                .last_watered
                .map(|last| (Utc::now().date_naive() - last).num_days() >= 2)
                .unwrap_or(age_days >= 2);
        let lines = render_tree_art_lines(
            stage,
            tree.seed,
            wilting,
            tree_area.width as usize,
            0.0,
            None,
        );
        bottom_align(frame, tree_area, lines);
        render_caption(
            frame,
            caption_area,
            &format!("{} · {age_days}d", stage.label()),
            tree.is_alive,
        );
        return;
    }

    render_centered_dim(frame, area, "no bonsai yet");
}

fn draw_aquarium_tab(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    if state.aquarium_fish().is_empty() {
        render_centered_dim(frame, area, "No aquarium to show here yet");
        return;
    }

    let cell = state.aquarium_cell();
    let mut slot = cell.borrow_mut();
    if slot.is_none() || state.aquarium_area().get() != area {
        state.aquarium_area().set(area);
        *slot = AquariumState::default_for_area(area)
            .ok()
            .map(|mut aquarium| {
                aquarium.set_active_creatures(state.aquarium_fish());
                aquarium
            });
    }

    if let Some(aquarium) = slot.as_mut() {
        aquarium.tick();
        aquarium_ui::draw(frame, area, aquarium);
    } else {
        render_centered_dim(frame, area, "aquarium unavailable");
    }
}

fn bottom_align(frame: &mut Frame, area: Rect, mut lines: Vec<Line<'static>>) {
    let top_pad = (area.height as usize).saturating_sub(lines.len());
    let mut out = vec![Line::from(""); top_pad];
    out.append(&mut lines);
    frame.render_widget(Paragraph::new(out), area);
}

fn render_caption(frame: &mut Frame, area: Rect, text: &str, alive: bool) {
    let style = if alive {
        Style::default().fg(theme::TEXT_DIM())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text.to_string(), style)).centered()),
        area,
    );
}

fn render_centered_dim(frame: &mut Frame, area: Rect, text: &str) {
    let top = (area.height as usize).saturating_sub(1) / 2;
    let mut lines = vec![Line::from(""); top];
    lines.push(
        Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ))
        .centered(),
    );
    frame.render_widget(Paragraph::new(lines), area);
}

fn sep() -> Span<'static> {
    Span::styled("   ·   ", Style::default().fg(theme::BORDER_DIM()))
}

fn format_two_cells(
    a: (&str, &str),
    b: (&str, &str),
    col_w: usize,
    label_style: Style,
    value_style: Style,
    sep_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (label, value)) in [a, b].into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("│ ", sep_style));
        }
        let label_padded = format!("{label:<9} ");
        let used = label_padded.chars().count() + value.chars().count();
        let pad = col_w.saturating_sub(used + if i == 0 { 2 } else { 0 });
        spans.push(Span::styled(label_padded, label_style));
        spans.push(Span::styled(value.to_string(), value_style));
        if i == 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
    }
    spans
}

fn format_created_at(created_at: &chrono::DateTime<Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
}

/// Render a `MM-DD` birthday as "7 March", appending a "today!" / "in N days"
/// hint when it is within a month.
fn format_birthday(birthday: &str) -> String {
    use late_core::models::birthday::{days_until, month_day_label, normalize_birthday};
    let Some(canonical) = normalize_birthday(birthday) else {
        return birthday.to_string();
    };
    let base = month_day_label(&canonical).unwrap_or_else(|| canonical.clone());
    match days_until(&canonical, Utc::now().date_naive()) {
        Some(0) => format!("{base} · today!"),
        Some(1) => format!("{base} · tomorrow"),
        Some(d) if d <= 30 => format!("{base} · in {d} days"),
        _ => base,
    }
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
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
