use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::{
    composer::{build_composer_lines_from_rows, composer_cursor_scroll_for_rows},
    theme,
};

use super::{
    data::country_label,
    state::{PickerKind, Row, WelcomeModalState},
};

const MODAL_WIDTH: u16 = 96;
const MODAL_HEIGHT: u16 = 34;

pub fn draw(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" late.sh ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // tagline
        Constraint::Length(1), // breathing room
        Constraint::Length(3), // help callout
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // body (rows | bio)
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // save CTA
        Constraint::Length(1), // footer keys
    ])
    .split(inner);

    draw_tagline(frame, layout[1]);
    draw_help_callout(frame, layout[3]);

    let body = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(layout[5]);

    draw_rows(frame, body[0], state);
    draw_bio_panel(frame, body[1], state);

    draw_save_cta(frame, layout[7], state);
    draw_footer(frame, layout[8]);

    if state.picker_open() {
        draw_picker(frame, popup, state);
    }
}

fn draw_tagline(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Tune your identity, vibes, and pings.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help_callout(frame: &mut Frame, area: Rect) {
    let inner_area = area.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER_DIM()));
    let inner = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            " ? ",
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Press ", Style::default().fg(theme::TEXT())),
        Span::styled(
            "?",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" for the late.sh tour", Style::default().fg(theme::TEXT())),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

fn draw_rows(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_heading("Identity"));
    lines.push(row_line(
        state,
        Row::Username,
        width,
        "Username",
        if state.editing_username() {
            if state.username_input().is_empty() {
                value_span("typing…", theme::AMBER())
            } else {
                value_span(format!("{}█", state.username_input()), theme::AMBER())
            }
        } else if state.draft().username.is_empty() {
            value_span("not set", theme::TEXT_FAINT())
        } else {
            value_span(state.draft().username.clone(), theme::TEXT_BRIGHT())
        },
    ));
    lines.push(row_line(
        state,
        Row::Bio,
        width,
        "Bio",
        if state.editing_bio() {
            value_span("editing…", theme::AMBER())
        } else if state.draft().bio.is_empty() {
            value_span("not set", theme::TEXT_FAINT())
        } else {
            value_span(preview_bio(state.draft().bio.as_str()), theme::TEXT())
        },
    ));

    lines.push(blank_line());
    lines.push(section_heading("Appearance"));
    lines.push(row_line(
        state,
        Row::Theme,
        width,
        "Theme",
        value_span(
            theme::label_for_id(state.draft().theme_id.as_deref().unwrap_or("late")).to_string(),
            theme::TEXT_BRIGHT(),
        ),
    ));
    lines.push(row_line(
        state,
        Row::BackgroundColor,
        width,
        "Background",
        toggle_span(state.draft().enable_background_color),
    ));

    lines.push(blank_line());
    lines.push(section_heading("Notifications"));
    lines.push(row_line(
        state,
        Row::DirectMessages,
        width,
        "DMs",
        toggle_span(has_kind(state, "dms")),
    ));
    lines.push(row_line(
        state,
        Row::Mentions,
        width,
        "@mentions",
        toggle_span(has_kind(state, "mentions")),
    ));
    lines.push(row_line(
        state,
        Row::GameEvents,
        width,
        "Game events",
        toggle_span(has_kind(state, "game_events")),
    ));
    lines.push(row_line(
        state,
        Row::Bell,
        width,
        "Bell",
        toggle_span(state.draft().notify_bell),
    ));
    lines.push(row_line(
        state,
        Row::Cooldown,
        width,
        "Cooldown",
        if state.draft().notify_cooldown_mins == 0 {
            value_span("off", theme::TEXT_FAINT())
        } else {
            value_span(
                format!("{} min", state.draft().notify_cooldown_mins),
                theme::TEXT_BRIGHT(),
            )
        },
    ));

    lines.push(blank_line());
    lines.push(section_heading("Location"));
    lines.push(row_line(
        state,
        Row::Country,
        width,
        "Country",
        value_with_picker_hint(country_label(state.draft().country.as_deref())),
    ));
    lines.push(row_line(
        state,
        Row::Timezone,
        width,
        "Timezone",
        value_with_picker_hint(
            state
                .draft()
                .timezone
                .clone()
                .unwrap_or_else(|| "not set".to_string()),
        ),
    ));

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_bio_panel(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let editing = state.editing_bio();
    let title = if editing {
        " Bio · editing (Alt+Enter newline) "
    } else {
        " Bio "
    };
    let block = Block::default()
        .title(title)
        .title_style(if editing {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if editing {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER_DIM()
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let composer = state.bio_input();
    if !editing && composer.text().is_empty() {
        let placeholder = vec![
            Line::from(""),
            Line::from(Span::styled(
                "A short multiline intro lives here.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Select Bio and press Enter to write one.",
                Style::default().fg(theme::TEXT_FAINT()),
            )),
        ];
        let placeholder_inner = inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        frame.render_widget(
            Paragraph::new(placeholder).wrap(Wrap { trim: false }),
            placeholder_inner,
        );
        return;
    }

    let lines = build_composer_lines_from_rows(
        composer.text(),
        composer.rows(),
        composer.cursor(),
        editing,
        editing,
    );
    let scroll =
        composer_cursor_scroll_for_rows(composer.rows(), composer.cursor(), inner.height as usize);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
}

fn draw_save_cta(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let selected =
        state.selected_row() == Row::Save && !state.editing_username() && !state.editing_bio();

    let (label, label_style, prefix_style) = if selected {
        (
            "  [ Press Enter to Save ]  ",
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme::AMBER_GLOW()),
        )
    } else {
        (
            "  [ Save profile ]  ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme::TEXT_DIM()),
        )
    };

    let line = Line::from(vec![
        Span::styled("  ", prefix_style),
        Span::styled(label, label_style),
        Span::styled(
            if selected {
                "   ↵ commits and closes"
            } else {
                "   ↓ to highlight, then ↵"
            },
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cycle  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" edit/apply  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}

fn draw_picker(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let popup = centered_rect(54, 20, area);
    frame.render_widget(Clear, popup);

    let title = match state.picker().kind {
        Some(PickerKind::Country) => " Pick Country ",
        Some(PickerKind::Timezone) => " Pick Timezone ",
        None => " Picker ",
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let search = Line::from(vec![
        Span::raw(" "),
        Span::styled("search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            if state.picker().query.is_empty() {
                "type to filter".to_string()
            } else {
                state.picker().query.clone()
            },
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
    ]);
    frame.render_widget(Paragraph::new(search), layout[1]);

    let entries: Vec<String> = match state.picker().kind {
        Some(PickerKind::Country) => state
            .filtered_countries()
            .into_iter()
            .map(|country| format!("[{}] {}", country.code, country.name))
            .collect(),
        Some(PickerKind::Timezone) => state
            .filtered_timezones()
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        None => Vec::new(),
    };

    let list_width = layout[2].width as usize;
    let visible_height = layout[2].height as usize;
    state.picker().visible_height.set(visible_height.max(1));
    let scroll = state.picker().scroll_offset;
    let end = (scroll + visible_height).min(entries.len());
    let mut lines = Vec::new();
    for (idx, entry) in entries[scroll..end].iter().enumerate() {
        let selected = scroll + idx == state.picker().selected_index;
        let (marker, fg, bg, modifier) = if selected {
            (
                "›",
                theme::AMBER_GLOW(),
                Some(theme::BG_HIGHLIGHT()),
                Modifier::BOLD,
            )
        } else {
            ("·", theme::TEXT(), None, Modifier::empty())
        };
        let mut style = Style::default().fg(fg).add_modifier(modifier);
        if let Some(bg) = bg {
            style = style.bg(bg);
        }
        let content = format!(" {marker} {entry}");
        let padded = pad_to_width(&content, list_width, bg.is_some());
        lines.push(Line::from(Span::styled(padded, style)));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no results",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), layout[2]);

    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" pick  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[3]);
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

fn blank_line() -> Line<'static> {
    Line::from("")
}

struct ValueSpan {
    text: String,
    style: Style,
}

fn value_span(text: impl Into<String>, color: ratatui::style::Color) -> ValueSpan {
    ValueSpan {
        text: text.into(),
        style: Style::default().fg(color),
    }
}

fn toggle_span(enabled: bool) -> ValueSpan {
    if enabled {
        ValueSpan {
            text: "● on".to_string(),
            style: Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        }
    } else {
        ValueSpan {
            text: "○ off".to_string(),
            style: Style::default().fg(theme::TEXT_FAINT()),
        }
    }
}

fn value_with_picker_hint(text: String) -> ValueSpan {
    ValueSpan {
        text: format!("{text}  …"),
        style: Style::default().fg(theme::TEXT_BRIGHT()),
    }
}

fn row_line(
    state: &WelcomeModalState,
    row: Row,
    width: usize,
    label: &str,
    value: ValueSpan,
) -> Line<'static> {
    let selected = state.selected_row() == row && !state.editing_username() && !state.editing_bio();

    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if selected {
        value.style.bg(theme::BG_SELECTION())
    } else {
        value.style
    };

    let prefix = format!(" {marker} ");
    let label_text = format!("{label:<13}");
    let mut used = prefix.chars().count() + label_text.chars().count() + value.text.chars().count();
    if used > width {
        used = width;
    }
    let padding = width.saturating_sub(used);
    let trailing = " ".repeat(padding);
    let trailing_style = if selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(trailing, trailing_style),
    ])
}

fn pad_to_width(text: &str, width: usize, _has_bg: bool) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let mut out = String::from(text);
    out.push_str(&" ".repeat(width - len));
    out
}

fn preview_bio(bio: &str) -> String {
    let mut lines = bio.lines();
    let first = lines.next().unwrap_or_default();
    let truncated: String = first.chars().take(40).collect();
    let suffix = if first.chars().count() > 40 || lines.next().is_some() {
        " …"
    } else {
        ""
    };
    format!("{truncated}{suffix}")
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
