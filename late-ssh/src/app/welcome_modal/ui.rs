use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::{
    composer::{build_composer_lines_from_rows, composer_cursor_scroll_for_rows},
    theme,
};

use super::{
    data::{country_flag, country_label},
    state::{PickerKind, Row, WelcomeModalState},
};

pub fn draw(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let popup = centered_rect(88, 28, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Welcome / Profile ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(3),
    ])
    .split(inner);

    let title_lines = vec![
        Line::from(vec![Span::styled(
            "  Set up your late.sh identity.",
            Style::default().fg(theme::TEXT()),
        )]),
        Line::from(Span::styled(
            "  This modal now owns profile settings. Save when it looks right.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    frame.render_widget(Paragraph::new(title_lines), layout[0]);

    let body = Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(layout[1]);

    draw_rows(frame, body[0], state);
    draw_side_panel(frame, body[1], state);

    let footer = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" edit/apply  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Alt+Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" newline in bio", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[2]);

    if state.picker_open() {
        draw_picker(frame, popup, state);
    }
}

fn draw_rows(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let mut lines = Vec::new();
    lines.push(row(
        state,
        Row::Username,
        "Username",
        if state.editing_username() {
            if state.username_input().is_empty() {
                "typing...".to_string()
            } else {
                format!("{}█", state.username_input())
            }
        } else if state.draft().username.is_empty() {
            "not set".to_string()
        } else {
            state.draft().username.clone()
        },
    ));

    lines.push(row(
        state,
        Row::Theme,
        "Theme",
        theme::label_for_id(state.draft().theme_id.as_deref().unwrap_or("late")).to_string(),
    ));
    lines.push(row(
        state,
        Row::BackgroundColor,
        "Background",
        on_off(state.draft().enable_background_color),
    ));
    lines.push(row(
        state,
        Row::DirectMessages,
        "DM notify",
        on_off(has_kind(state, "dms")),
    ));
    lines.push(row(
        state,
        Row::Mentions,
        "@mention notify",
        on_off(has_kind(state, "mentions")),
    ));
    lines.push(row(
        state,
        Row::GameEvents,
        "Game notify",
        on_off(has_kind(state, "game_events")),
    ));
    lines.push(row(
        state,
        Row::Bell,
        "Bell",
        on_off(state.draft().notify_bell),
    ));
    lines.push(row(
        state,
        Row::Cooldown,
        "Cooldown",
        if state.draft().notify_cooldown_mins == 0 {
            "Off".to_string()
        } else {
            format!("{} min", state.draft().notify_cooldown_mins)
        },
    ));
    lines.push(row(
        state,
        Row::Country,
        "Country",
        country_label(state.draft().country.as_deref()),
    ));
    lines.push(row(
        state,
        Row::Timezone,
        "Timezone",
        state
            .draft()
            .timezone
            .clone()
            .unwrap_or_else(|| "Not set".to_string()),
    ));
    lines.push(row(
        state,
        Row::Bio,
        "Bio",
        if state.editing_bio() {
            "editing...".to_string()
        } else if state.draft().bio.is_empty() {
            "Not set".to_string()
        } else {
            preview_bio(state.draft().bio.as_str())
        },
    ));
    lines.push(row(state, Row::Save, "Save", "Persist profile".to_string()));

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_side_panel(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let block = Block::default()
        .title(" Bio (Alt+Enter newline) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.editing_bio() {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let composer = state.bio_input();
    let lines = build_composer_lines_from_rows(
        composer.text(),
        composer.rows(),
        composer.cursor(),
        state.editing_bio(),
        state.editing_bio(),
    );
    let scroll =
        composer_cursor_scroll_for_rows(composer.rows(), composer.cursor(), inner.height as usize);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);

    if !state.editing_bio() && composer.text().is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Add a short multiline intro.",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            inner,
        );
    }
}

fn draw_picker(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let popup = centered_rect(52, 18, area);
    frame.render_widget(Clear, popup);

    let title = match state.picker().kind {
        Some(PickerKind::Country) => " Pick Country ",
        Some(PickerKind::Timezone) => " Pick Timezone ",
        None => " Picker ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let search = Line::from(vec![
        Span::styled("  search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            if state.picker().query.is_empty() {
                "type to filter".to_string()
            } else {
                state.picker().query.clone()
            },
            Style::default().fg(theme::TEXT()),
        ),
    ]);
    frame.render_widget(Paragraph::new(search), layout[0]);

    let entries: Vec<String> = match state.picker().kind {
        Some(PickerKind::Country) => state
            .filtered_countries()
            .into_iter()
            .map(|country| {
                let flag = country_flag(country.code).unwrap_or_default();
                format!("{flag} {} ({})", country.name, country.code)
            })
            .collect(),
        Some(PickerKind::Timezone) => state
            .filtered_timezones()
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        None => Vec::new(),
    };

    let visible_height = layout[1].height as usize;
    state.picker().visible_height.set(visible_height.max(1));
    let scroll = state.picker().scroll_offset;
    let end = (scroll + visible_height).min(entries.len());
    let mut lines = Vec::new();
    for (idx, entry) in entries[scroll..end].iter().enumerate() {
        let selected = scroll + idx == state.picker().selected_index;
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let marker = if selected { "›" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {marker} "),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::styled(entry.clone(), style),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No results",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), layout[1]);

    let footer = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" pick  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[2]);
}

fn row(state: &WelcomeModalState, row: Row, label: &str, value: String) -> Line<'static> {
    let selected = state.selected_row() == row && !state.editing_username() && !state.editing_bio();
    let marker = if selected { "›" } else { " " };
    let label_style = if selected {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if selected {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT())
    };
    Line::from(vec![
        Span::styled(
            format!(" {marker} "),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(format!("{label:<14}"), label_style),
        Span::styled(value, value_style),
    ])
}

fn preview_bio(bio: &str) -> String {
    let mut lines = bio.lines();
    let first = lines.next().unwrap_or_default();
    if lines.next().is_some() {
        format!("{first} …")
    } else {
        first.to_string()
    }
}

fn on_off(enabled: bool) -> String {
    if enabled {
        "On".to_string()
    } else {
        "Off".to_string()
    }
}

fn has_kind(state: &WelcomeModalState, kind: &str) -> bool {
    state.draft().notify_kinds.iter().any(|value| value == kind)
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
