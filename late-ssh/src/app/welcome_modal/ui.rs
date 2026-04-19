use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::{
    composer::{
        build_composer_lines_from_rows, composer_cursor_scroll_for_rows,
        composer_line_count_for_rows,
    },
    theme,
};

use super::{
    data::country_label,
    state::{BIO_MAX_LEN, PickerKind, Row, WelcomeModalState},
};

pub const MODAL_WIDTH: u16 = 96;
pub const MODAL_HEIGHT: u16 = 34;
const SETTINGS_COLUMN_WIDTH: u16 = 38;
const BODY_COLUMN_GAP: u16 = 1;

/// Width the bio composer should wrap at, given the modal's rendered width.
/// The bio editor lives in the right-hand pane:
/// modal inner (-2 borders) - settings column - gutter - bio block borders (-2)
/// - composer's leading space (-1).
pub fn bio_text_width(modal_width: u16) -> usize {
    modal_width
        .saturating_sub(2)
        .saturating_sub(SETTINGS_COLUMN_WIDTH)
        .saturating_sub(BODY_COLUMN_GAP)
        .saturating_sub(3)
        .max(24) as usize
}

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

    draw_body(frame, layout[5], state);

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

fn draw_body(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let columns = Layout::horizontal([
        Constraint::Length(SETTINGS_COLUMN_WIDTH),
        Constraint::Length(BODY_COLUMN_GAP),
        Constraint::Min(24),
    ])
    .split(area);

    draw_settings_column(frame, columns[0], state);
    draw_bio_pane(frame, columns[2], state);
}

fn draw_settings_column(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // Identity heading
        Constraint::Length(1), // Username row
        Constraint::Length(1), // Bio row
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // Appearance heading
        Constraint::Length(1), // Theme
        Constraint::Length(1), // Background
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // Location heading
        Constraint::Length(1), // Country
        Constraint::Length(1), // Timezone
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // Notifications heading
        Constraint::Length(1), // DMs
        Constraint::Length(1), // Mentions
        Constraint::Length(1), // Game events
        Constraint::Length(1), // Bell
        Constraint::Length(1), // Cooldown
    ])
    .split(area);

    let width = area.width as usize;

    frame.render_widget(Paragraph::new(section_heading("Identity")), sections[0]);
    frame.render_widget(
        Paragraph::new(row_line(
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
        )),
        sections[1],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Bio,
            width,
            "Bio",
            bio_summary_value(state),
        )),
        sections[2],
    );

    frame.render_widget(Paragraph::new(section_heading("Appearance")), sections[4]);
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Theme,
            width,
            "Theme",
            value_span(
                theme::label_for_id(state.draft().theme_id.as_deref().unwrap_or("late"))
                    .to_string(),
                theme::TEXT_BRIGHT(),
            ),
        )),
        sections[5],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::BackgroundColor,
            width,
            "Background",
            toggle_span(state.draft().enable_background_color),
        )),
        sections[6],
    );

    frame.render_widget(Paragraph::new(section_heading("Location")), sections[8]);
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Country,
            width,
            "Country",
            value_with_picker_hint(country_label(state.draft().country.as_deref())),
        )),
        sections[9],
    );
    frame.render_widget(
        Paragraph::new(row_line(
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
        )),
        sections[10],
    );

    frame.render_widget(
        Paragraph::new(section_heading("Notifications")),
        sections[12],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::DirectMessages,
            width,
            "DMs",
            toggle_span(has_kind(state, "dms")),
        )),
        sections[13],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Mentions,
            width,
            "@mentions",
            toggle_span(has_kind(state, "mentions")),
        )),
        sections[14],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::GameEvents,
            width,
            "Game events",
            toggle_span(has_kind(state, "game_events")),
        )),
        sections[15],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Bell,
            width,
            "Bell",
            toggle_span(state.draft().notify_bell),
        )),
        sections[16],
    );
    frame.render_widget(
        Paragraph::new(row_line(
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
        )),
        sections[17],
    );
}

fn draw_bio_pane(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let editing = state.editing_bio();
    let selected = state.selected_row() == Row::Bio && !state.editing_username();

    let title = if editing {
        " Bio · Esc/Enter save · Alt+Enter newline "
    } else if state.draft().bio.is_empty() {
        " Bio · Enter to write "
    } else {
        " Bio · Enter to edit "
    };

    let (border_color, title_color) = if editing {
        (theme::BORDER_ACTIVE(), theme::AMBER_GLOW())
    } else if selected {
        (theme::AMBER_DIM(), theme::AMBER())
    } else {
        (theme::BORDER_DIM(), theme::TEXT_DIM())
    };

    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(6)]).split(inner);
    draw_bio_intro(frame, sections[0], state);
    draw_bio_content(frame, sections[1], state);
}

fn draw_bio_intro(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let bio = state.bio_input();
    let char_count = bio.text().chars().count();
    let line_count = composer_line_count_for_rows(bio.text(), bio.rows());
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let lead = if state.editing_bio() {
        Line::from(vec![
            Span::styled(" Move with arrows. ", Style::default().fg(theme::AMBER())),
            Span::styled("Keep it readable.", Style::default().fg(theme::TEXT_DIM())),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " Keep it skimmable. ",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Lead with the useful bits.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(lead), rows[0]);

    let subline = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "Role, GitHub, projects, links.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(subline), rows[1]);

    let stats = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{char_count}/{BIO_MAX_LEN} chars"),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{line_count} lines"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(stats), rows[2]);
}

fn draw_bio_content(frame: &mut Frame, area: Rect, state: &WelcomeModalState) {
    let editing = state.editing_bio();

    let composer = state.bio_input();
    if composer.text().is_empty() {
        frame.render_widget(
            Paragraph::new(bio_placeholder_lines(editing)).wrap(Wrap { trim: false }),
            area,
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
    let scroll = if editing {
        composer_cursor_scroll_for_rows(composer.rows(), composer.cursor(), area.height as usize)
    } else {
        0
    };
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}

fn bio_placeholder_lines(editing: bool) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());

    if editing {
        return vec![Line::from(vec![
            Span::raw(" "),
            Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
            Span::styled(" Start with a role, link, or one-line intro.", dim),
        ])];
    }

    vec![Line::from(vec![Span::styled(
        " Build a bio people can skim in five seconds.",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )])]
}

fn bio_summary_value(state: &WelcomeModalState) -> ValueSpan {
    let bio = state.bio_input();
    let char_count = bio.text().chars().count();
    let line_count = composer_line_count_for_rows(bio.text(), bio.rows());

    if state.editing_bio() {
        return value_span(
            format!("editing {char_count}/{BIO_MAX_LEN}"),
            theme::AMBER(),
        );
    }

    if bio.text().is_empty() {
        return value_span("empty - press Enter", theme::TEXT_FAINT());
    }

    value_span(
        format!("{line_count} lines - {char_count}/{BIO_MAX_LEN}"),
        theme::TEXT_BRIGHT(),
    )
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
                "   highlight Save, then ↵"
            },
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("j/k ↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cycle  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" edit/apply  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
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
