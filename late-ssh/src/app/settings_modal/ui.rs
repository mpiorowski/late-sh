use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::{markdown::render_body_to_lines, theme};

use super::{
    data::country_label,
    state::{BIO_MAX_LEN, PickerKind, Row, SettingsModalState, Tab, ThemeTreeRow},
};

pub const MODAL_WIDTH: u16 = 96;
pub const MODAL_HEIGHT: u16 = 34;

pub fn draw(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Settings ")
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
        Constraint::Length(1), // tabs
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // body
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_tabs(frame, layout[1], state.selected_tab());

    match state.selected_tab() {
        Tab::Settings => draw_settings_tab(frame, layout[3], state),
        Tab::Themes => draw_themes_tab(frame, layout[3], state),
        Tab::Bio => draw_bio_tab(frame, layout[3], state),
        Tab::Favorites => draw_favorites_tab(frame, layout[3], state),
    }

    draw_footer(frame, layout[4], state.selected_tab(), state.editing_bio());

    if state.picker_open() {
        draw_picker(frame, popup, state);
    }
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: Tab) {
    let mut spans = vec![Span::raw("  ")];
    for tab in Tab::ALL {
        let active = tab == selected;
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, tab: Tab, editing_bio: bool) {
    let mut spans = vec![Span::raw("  ")];
    match (tab, editing_bio) {
        (Tab::Bio, true) => {
            spans.extend([
                Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" save & preview  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Alt+Enter", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" newline  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(
                    " save & switch tabs",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]);
        }
        (Tab::Bio, false) => {
            spans.extend([
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" edit  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Settings, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" cycle  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" edit/apply  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Themes, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" preview  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close/open  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Favorites, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("J/K", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" reorder  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("d", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" remove  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" add  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_themes_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // summary
        Constraint::Length(1), // breathing
        Constraint::Min(4),    // tree
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(section_heading("Theme browser")),
        sections[0],
    );

    let active_id = state.draft().theme_id.as_deref().unwrap_or("late");
    let active_preview = theme::preview_for_id(active_id);
    let summary = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            theme::label_for_id(active_id).to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default().fg(theme::TEXT_DIM())),
        swatch(active_preview.bg_canvas),
        swatch(active_preview.bg_selection),
        swatch(active_preview.border_active),
        swatch(active_preview.amber),
        swatch(active_preview.chat_author),
        swatch(active_preview.mention),
        swatch(active_preview.success),
        swatch(active_preview.error),
        Span::styled(
            format!("   {}", theme::color_to_hex(active_preview.border_active)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(summary), sections[1]);

    let tree_area = sections[3];
    let width = tree_area.width as usize;
    let visible_height = tree_area.height as usize;
    state.set_theme_visible_height(visible_height.max(1));

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (row_idx, row) in state
        .theme_tree_rows()
        .into_iter()
        .enumerate()
        .skip(state.theme_scroll_offset())
    {
        if lines.len() >= visible_height {
            break;
        }

        let selected = row_idx == state.theme_selected_row();
        match row {
            ThemeTreeRow::Group { group, collapsed } => {
                lines.push(theme_group_line(group, collapsed, selected, width));
            }
            ThemeTreeRow::Theme {
                option_index,
                last_in_group,
            } => {
                lines.push(theme_option_line(
                    theme::OPTIONS[option_index],
                    selected,
                    last_in_group,
                    width,
                ));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), tree_area);
}

fn theme_group_line(
    group: theme::ThemeGroup,
    collapsed: bool,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let symbol = if collapsed { "▸" } else { "▾" };
    let text = format!(" {marker} {symbol} {}", group.label());
    let padding = width.saturating_sub(text.chars().count());
    let style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    };
    let trailing_style = if selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(text, style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn theme_option_line(
    option: theme::ThemeOption,
    selected: bool,
    last_in_group: bool,
    width: usize,
) -> Line<'static> {
    let preview = theme::preview_for_option(option);
    let marker = if selected { "›" } else { " " };
    let branch = if last_in_group { "└─" } else { "├─" };
    let prefix = format!(" {marker} {branch} ");
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
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let id_style = if selected {
        Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };
    let swatches = [
        preview.bg_canvas,
        preview.bg_selection,
        preview.border_active,
        preview.text,
        preview.text_bright,
        preview.amber,
        preview.chat_author,
        preview.mention,
    ];
    let id_text = format!("  {}", option.id);
    let used = prefix.chars().count()
        + option.label.chars().count()
        + id_text.chars().count()
        + 2
        + (swatches.len() * 2);
    let padding = width.saturating_sub(used);
    let mut spans = vec![
        Span::styled(prefix, prefix_style),
        Span::styled(option.label.to_string(), label_style),
        Span::styled(id_text, id_style),
        Span::styled(" ".repeat(padding + 2), trailing_style),
    ];
    for color in swatches {
        spans.push(swatch(color));
    }
    Line::from(spans)
}

fn swatch(color: ratatui::style::Color) -> Span<'static> {
    Span::styled("  ", Style::default().bg(color))
}

fn draw_settings_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // Identity heading
        Constraint::Length(1), // Username row
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // Appearance heading
        Constraint::Length(1), // Theme
        Constraint::Length(1), // Background
        Constraint::Length(1), // Stream + vote
        Constraint::Length(1), // Right sidebar
        Constraint::Length(1), // Games sidebar
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
        Constraint::Length(1), // Format
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
                let typed = state.username_input().lines().join("");
                if typed.is_empty() {
                    value_span("typing…", theme::AMBER())
                } else {
                    value_span(format!("{}█", typed), theme::AMBER())
                }
            } else if state.draft().username.is_empty() {
                value_span("not set", theme::TEXT_FAINT())
            } else {
                value_span(state.draft().username.clone(), theme::TEXT_BRIGHT())
            },
        )),
        sections[1],
    );

    frame.render_widget(Paragraph::new(section_heading("Appearance")), sections[3]);
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
        sections[4],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::BackgroundColor,
            width,
            "Background",
            toggle_span(state.draft().enable_background_color),
        )),
        sections[5],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::DashboardHeader,
            width,
            "Stream + vote",
            toggle_span(state.draft().show_dashboard_header),
        )),
        sections[6],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::RightSidebar,
            width,
            "Right sidebar",
            toggle_span(state.draft().show_right_sidebar),
        )),
        sections[7],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::GamesSidebar,
            width,
            "Games sidebar",
            toggle_span(state.draft().show_games_sidebar),
        )),
        sections[8],
    );

    frame.render_widget(Paragraph::new(section_heading("Location")), sections[10]);
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Country,
            width,
            "Country",
            value_with_picker_hint(country_label(state.draft().country.as_deref())),
        )),
        sections[11],
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
        sections[12],
    );

    frame.render_widget(
        Paragraph::new(section_heading("Notifications")),
        sections[14],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::DirectMessages,
            width,
            "DMs",
            toggle_span(has_kind(state, "dms")),
        )),
        sections[15],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Mentions,
            width,
            "@mentions",
            toggle_span(has_kind(state, "mentions")),
        )),
        sections[16],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::GameEvents,
            width,
            "Game events",
            toggle_span(has_kind(state, "game_events")),
        )),
        sections[17],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Bell,
            width,
            "Bell",
            toggle_span(state.draft().notify_bell),
        )),
        sections[18],
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
        sections[19],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::NotifyFormat,
            width,
            "Format",
            value_span(
                notify_format_label(state.draft().notify_format.as_deref()),
                theme::TEXT_BRIGHT(),
            ),
        )),
        sections[20],
    );
}

fn notify_format_label(format: Option<&str>) -> &'static str {
    match format.unwrap_or("both") {
        "osc777" => "OSC 777",
        "osc9" => "OSC 9",
        _ => "both (OSC 777 + OSC 9)",
    }
}

fn draw_bio_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let editing = state.editing_bio();
    let bio = state.bio_input();
    let text = bio.lines().join("\n");
    let char_count = text.chars().count();

    // One-line header: char count + hint.
    let sections = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Length(1), // breathing
        Constraint::Min(4),    // editor OR preview
    ])
    .split(area);

    let header_style_count = Style::default().fg(theme::TEXT_BRIGHT());
    let header_style_dim = Style::default().fg(theme::TEXT_DIM());
    let header = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{char_count}/{BIO_MAX_LEN}"),
            if editing {
                header_style_count.add_modifier(Modifier::BOLD)
            } else {
                header_style_count
            },
        ),
        Span::styled("   chars", header_style_dim),
    ]);
    frame.render_widget(Paragraph::new(header), sections[0]);

    let body = sections[2];
    let padded = body.inner(Margin::new(2, 0));

    if editing {
        frame.render_widget(bio, padded);
        return;
    }

    // Not editing → render the draft as markdown. Empty bio shows a nudge.
    let draft_text = state.draft().bio.as_str();
    if draft_text.trim().is_empty() {
        let hint = Line::from(vec![Span::styled(
            "Press ↵ to write your bio. Markdown is supported.",
            Style::default().fg(theme::TEXT_DIM()),
        )]);
        frame.render_widget(Paragraph::new(hint).wrap(Wrap { trim: false }), padded);
        return;
    }

    let wrap_width = padded.width.saturating_sub(0) as usize;
    let lines = render_body_to_lines(
        draft_text,
        wrap_width,
        Span::raw(""),
        Style::default().fg(theme::TEXT()),
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), padded);
}

fn draw_favorites_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing
        Constraint::Min(4),    // body
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(section_heading("Favorite rooms")),
        sections[0],
    );

    let hint = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Pin rooms to the dashboard quick-switch strip ([ / ]).",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(hint), sections[1]);

    let body_width = sections[3].width as usize;
    let favorites = state.favorites();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(favorites.len() + 1);

    for (idx, room_id) in favorites.iter().enumerate() {
        let selected = state.favorites_index() == idx;
        let label_text = state
            .room_label(*room_id)
            .map(ToString::to_string)
            .unwrap_or_else(|| "(unknown room)".to_string());
        let position_text = format!("{:>2}. ", idx + 1);
        let label_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        let position_style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        let marker = if selected { "›" } else { " " };
        let prefix_style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        let prefix = format!(" {marker} ");
        let used =
            prefix.chars().count() + position_text.chars().count() + label_text.chars().count();
        let padding = body_width.saturating_sub(used);
        let trailing = " ".repeat(padding);
        let trailing_style = if selected {
            Style::default().bg(theme::BG_SELECTION())
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, prefix_style),
            Span::styled(position_text, position_style),
            Span::styled(label_text, label_style),
            Span::styled(trailing, trailing_style),
        ]));
    }

    // Trailing "Add favorite…" row. Highlighted like a favorite row when
    // selected so the visual language is consistent.
    let add_selected = state.favorites_index_is_add_row();
    let add_text = if state.available_rooms().len() == favorites.len() {
        "(no more rooms to add — join one in chat first)"
    } else {
        "+ Add favorite room…"
    };
    let add_style = if add_selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let marker = if add_selected { "›" } else { " " };
    let prefix_style = if add_selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let prefix = format!(" {marker} ");
    let used = prefix.chars().count() + add_text.chars().count();
    let padding = body_width.saturating_sub(used);
    let trailing_style = if add_selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(add_text.to_string(), add_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ]));

    frame.render_widget(Paragraph::new(lines), sections[3]);
}

fn draw_picker(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(54, 20, area);
    frame.render_widget(Clear, popup);

    let title = match state.picker().kind {
        Some(PickerKind::Country) => " Pick Country ",
        Some(PickerKind::Timezone) => " Pick Timezone ",
        Some(PickerKind::Room) => " Pick Room ",
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
        Some(PickerKind::Room) => state
            .filtered_rooms()
            .into_iter()
            .map(|room| room.label.clone())
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
    state: &SettingsModalState,
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
    let label_text = format!("{label:<16}");
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

fn has_kind(state: &SettingsModalState, kind: &str) -> bool {
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
