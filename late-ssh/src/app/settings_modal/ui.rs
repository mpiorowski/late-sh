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
    gem::{GemPosition, GemState, MoveDirection},
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

    draw_tabs(frame, layout[1], state);

    match state.selected_tab() {
        Tab::Settings => draw_settings_tab(frame, layout[3], state),
        Tab::Themes => draw_themes_tab(frame, layout[3], state),
        Tab::Bio => draw_bio_tab(frame, layout[3], state),
        Tab::Favorites => draw_favorites_tab(frame, layout[3], state),
        Tab::Account => draw_account_tab(frame, layout[3], state),
        Tab::Feeds => draw_feeds_tab(frame, layout[3], state),
        Tab::Special => draw_special_tab(frame, layout[3], state),
    }

    draw_footer(frame, layout[4], state.selected_tab(), state.editing_bio());

    if state.picker_open() {
        draw_picker(frame, popup, state);
    }
    if state.delete_account_dialog().open() {
        draw_delete_account_dialog(frame, popup, state);
    }
}

fn draw_tabs(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let selected = state.selected_tab();
    let mut spans = vec![Span::raw("  ")];
    for tab in state.visible_tabs() {
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
                Span::styled("Alt+Enter/Ctrl+J", Style::default().fg(theme::AMBER_DIM())),
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
        (Tab::Special, _) => {
            spans.extend([
                Span::styled("←→ ↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" toggle  ", Style::default().fg(theme::TEXT_DIM())),
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
        (Tab::Account, _) => {
            spans.extend([
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" open confirm  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Feeds, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵/a", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" add  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("d", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" remove  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("r", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" refresh  ", Style::default().fg(theme::TEXT_DIM())),
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

    let active_id = state
        .draft()
        .theme_id
        .as_deref()
        .unwrap_or(theme::DEFAULT_ID);
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
        Constraint::Length(1), // IDE row
        Constraint::Length(1), // Terminal row
        Constraint::Length(1), // OS row
        Constraint::Length(1), // Languages row
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // Appearance heading
        Constraint::Length(1), // Theme
        Constraint::Length(1), // Background
        Constraint::Length(1), // Stream + vote
        Constraint::Length(1), // Right sidebar
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
                    value_span("█", theme::AMBER())
                } else {
                    value_span(
                        text_with_caret(&typed, state.username_input().cursor().1),
                        theme::AMBER(),
                    )
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
            Row::Ide,
            width,
            "IDE",
            system_field_value(state, Row::Ide, state.draft().ide.clone()),
        )),
        sections[2],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Terminal,
            width,
            "Terminal",
            system_field_value(state, Row::Terminal, state.draft().terminal.clone()),
        )),
        sections[3],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Os,
            width,
            "OS",
            system_field_value(state, Row::Os, state.draft().os.clone()),
        )),
        sections[4],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Langs,
            width,
            "Langs",
            system_field_value(
                state,
                Row::Langs,
                (!state.draft().langs.is_empty()).then(|| format_lang_tags(&state.draft().langs)),
            ),
        )),
        sections[5],
    );

    frame.render_widget(Paragraph::new(section_heading("Appearance")), sections[7]);
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Theme,
            width,
            "Theme",
            value_span(
                theme::label_for_id(
                    state
                        .draft()
                        .theme_id
                        .as_deref()
                        .unwrap_or(theme::DEFAULT_ID),
                )
                .to_string(),
                theme::TEXT_BRIGHT(),
            ),
        )),
        sections[8],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::BackgroundColor,
            width,
            "Background",
            toggle_span(state.draft().enable_background_color),
        )),
        sections[9],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::DashboardHeader,
            width,
            "Stream + vote",
            toggle_span(state.draft().show_dashboard_header),
        )),
        sections[10],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::RightSidebar,
            width,
            "Right sidebar",
            toggle_span(state.draft().show_right_sidebar),
        )),
        sections[11],
    );
    frame.render_widget(Paragraph::new(section_heading("Location")), sections[13]);
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Country,
            width,
            "Country",
            value_with_picker_hint(country_label(state.draft().country.as_deref())),
        )),
        sections[14],
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
        sections[15],
    );

    frame.render_widget(
        Paragraph::new(section_heading("Notifications")),
        sections[17],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::DirectMessages,
            width,
            "DMs",
            toggle_span(has_kind(state, "dms")),
        )),
        sections[18],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Mentions,
            width,
            "@mentions",
            toggle_span(has_kind(state, "mentions")),
        )),
        sections[19],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::GameEvents,
            width,
            "Game events",
            toggle_span(has_kind(state, "game_events")),
        )),
        sections[20],
    );
    frame.render_widget(
        Paragraph::new(row_line(
            state,
            Row::Bell,
            width,
            "Bell",
            toggle_span(state.draft().notify_bell),
        )),
        sections[21],
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
        sections[22],
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
        sections[23],
    );
}

fn draw_special_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    // Reserve a 7-line strip at the bottom: 6 for the shining grand gem
    // (5-line body + 1 row of sparkles above) and 1 row of padding off the
    // dialog's bottom border.
    const GEM_STRIP_HEIGHT: u16 = 7;
    let gem_strip_height = GEM_STRIP_HEIGHT.min(area.height.saturating_sub(4));

    let sections = Layout::vertical([
        Constraint::Length(1),                // heading
        Constraint::Length(1),                // hint
        Constraint::Length(1),                // breathing
        Constraint::Length(1),                // toggle row
        Constraint::Min(0),                   // flex spacer
        Constraint::Length(gem_strip_height), // gem
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Special")), sections[0]);

    let hint = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Power-user toggles unlocked by completing your profile.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(hint), sections[1]);

    let width = area.width as usize;
    let label = "Show settings on connect";
    let value = toggle_span(state.draft().show_settings_on_connect);

    let prefix_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .bg(theme::BG_SELECTION())
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_SELECTION())
        .add_modifier(Modifier::BOLD);
    let value_style = value.style.bg(theme::BG_SELECTION());
    let trailing_style = Style::default().bg(theme::BG_SELECTION());

    let prefix = " › ".to_string();
    let label_text = format!("{label:<26}");
    let mut used = prefix.chars().count() + label_text.chars().count() + value.text.chars().count();
    if used > width {
        used = width;
    }
    let trailing = " ".repeat(width.saturating_sub(used));

    let line = Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(trailing, trailing_style),
    ]);
    frame.render_widget(Paragraph::new(line), sections[3]);

    if gem_strip_height > 0 {
        // Pad 2 cols off each side and lift the gem 1 row off the bottom
        // border so it doesn't crowd the dialog frame.
        const PAD_X: u16 = 2;
        const PAD_BOTTOM: u16 = 1;
        let strip = sections[5];
        let pad_x = PAD_X.min(strip.width / 2);
        let pad_bottom = PAD_BOTTOM.min(strip.height);
        let gem_area = Rect::new(
            strip.x + pad_x,
            strip.y,
            strip.width.saturating_sub(pad_x * 2),
            strip.height.saturating_sub(pad_bottom),
        );
        if gem_area.width > 0 && gem_area.height > 0 {
            draw_gem(frame, gem_area, state.gem());
        } else {
            state.gem().hit_area.set(None);
        }
    } else {
        state.gem().hit_area.set(None);
    }
}

fn draw_account_tab(frame: &mut Frame, area: Rect, _state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // breathing
        Constraint::Length(1), // button
        Constraint::Length(1), // description
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Account")), sections[0]);

    let width = area.width as usize;
    let label = "Delete Account";
    let prefix = " › ";
    let used = prefix.chars().count() + label.chars().count();
    let trailing = " ".repeat(width.saturating_sub(used));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(theme::ERROR())
                    .bg(theme::BG_SELECTION())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                label,
                Style::default()
                    .fg(theme::ERROR())
                    .bg(theme::BG_SELECTION())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(trailing, Style::default().bg(theme::BG_SELECTION())),
        ])),
        sections[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                "Delete your own account (cannot be undone!)",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[3],
    );
}

fn draw_feeds_tab(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing
        Constraint::Min(4),    // list
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Feeds")), sections[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "RSS/Atom entries stay private until you share them from Chat > feeds.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let width = sections[3].width as usize;
    let mut lines = Vec::new();
    for (idx, feed) in state.feeds().iter().enumerate() {
        lines.push(feed_row_line(
            idx == state.feed_index() && !state.editing_feed_url(),
            width,
            feed_display_title(feed),
            feed.url.as_str(),
            feed.last_error.as_deref(),
        ));
    }
    lines.push(feed_add_line(
        state.feed_index_is_add_row() && !state.editing_feed_url(),
        state.editing_feed_url(),
        width,
        state,
    ));

    frame.render_widget(Paragraph::new(lines), sections[3]);
}

fn feed_display_title(feed: &late_core::models::rss_feed::RssFeed) -> String {
    let title = feed.title.trim();
    if title.is_empty() {
        "untitled feed".to_string()
    } else {
        title.to_string()
    }
}

fn feed_row_line(
    selected: bool,
    width: usize,
    title: String,
    url: &str,
    error: Option<&str>,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let title_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let url_style = if selected {
        Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let error_style = if selected {
        Style::default()
            .fg(theme::ERROR())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::ERROR())
    };
    let trailing_style = if selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let title_text = format!("{title:<28}  ");
    let status_text = error
        .map(|err| format!("  error: {err}"))
        .unwrap_or_default();
    let used = prefix.chars().count()
        + title_text.chars().count()
        + url.chars().count()
        + status_text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(title_text, title_style),
        Span::styled(url.to_string(), url_style),
        Span::styled(status_text, error_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn feed_add_line(
    selected: bool,
    editing: bool,
    width: usize,
    state: &SettingsModalState,
) -> Line<'static> {
    let active = selected || editing;
    let marker = if active { "›" } else { " " };
    let prefix_style = if active {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if active {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let (text, text_style) = if editing {
        let typed = state.feed_url_input().lines().join("");
        let display = if typed.is_empty() {
            "█".to_string()
        } else {
            text_with_caret(&typed, state.feed_url_input().cursor().1)
        };
        (
            display,
            Style::default()
                .fg(theme::AMBER())
                .bg(theme::BG_SELECTION()),
        )
    } else if active {
        (
            "+ Add feed…".to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "+ Add feed…".to_string(),
            Style::default().fg(theme::AMBER_DIM()),
        )
    };

    let used = prefix.chars().count() + text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(text, text_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

/// Layout note: `area` is the 6-line strip reserved at the bottom of the
/// Special tab. The small gem hugs a corner; the grand gem is centered.
/// The gem's screen-coordinate rect is stashed back on `gem.hit_area` so the
/// input handler can do mouse hit testing.
fn draw_gem(frame: &mut Frame, area: Rect, gem: &GemState) {
    if gem.evolved() {
        draw_grand_gem(frame, area, gem);
    } else {
        draw_small_gem(frame, area, gem);
    }
}

fn draw_small_gem(frame: &mut Frame, area: Rect, gem: &GemState) {
    const SMALL_W: u16 = 3;
    const SMALL_H: u16 = 3;
    if area.width < SMALL_W || area.height < SMALL_H {
        gem.hit_area.set(None);
        return;
    }
    let style = Style::default().fg(gem.color());
    let mid = match gem.brand() {
        0 => "\\ /".to_string(),
        n => format!("\\{}/", n),
    };
    let rows = ["___", mid.as_str(), " ' "];

    let x = match gem.position() {
        GemPosition::Left => area.x,
        GemPosition::Right => area.x + area.width.saturating_sub(SMALL_W),
    };
    let y_start = area.y + area.height.saturating_sub(SMALL_H);

    for (i, row) in rows.iter().enumerate() {
        let row_rect = Rect::new(x, y_start + i as u16, SMALL_W, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(row.to_string(), style))),
            row_rect,
        );
    }

    if let Some(direction) = gem.last_move() {
        draw_speed_trail(frame, area, x, y_start, direction, style);
    }

    gem.hit_area
        .set(Some(Rect::new(x, y_start, SMALL_W, SMALL_H)));
}

/// Speed-trail wisps. Rendered on the gem's middle and bottom rows,
/// extending away from the gem in the direction it just came from.
fn draw_speed_trail(
    frame: &mut Frame,
    area: Rect,
    gem_x: u16,
    gem_y_start: u16,
    direction: MoveDirection,
    style: Style,
) {
    // The two rows of trail ASCII, side-aligned with the gem so position
    // math stays in one place. Each pair is `(mid_row, bottom_row)`.
    let (mid, bottom) = match direction {
        MoveDirection::Leftward => ("  .:`  .:    .", "   ':.. ':..  ':..  ':..  :..  ..  .   ."),
        MoveDirection::Rightward => (
            ".    :.  `:.  ",
            "   .   .  ..  ..:  ..:'  ..:'  ..:' ..:'    ",
        ),
    };

    let mid_y = gem_y_start + 1;
    let bottom_y = gem_y_start + 2;

    let area_left = area.x;
    let area_right = area.x + area.width;

    for (text, y) in [(mid, mid_y), (bottom, bottom_y)] {
        let len = text.chars().count() as u16;
        let (x, render_text): (u16, String) = match direction {
            MoveDirection::Leftward => {
                // Trail starts immediately to the right of the gem; clip
                // anything that would spill past the area's right edge.
                let start = gem_x + 3;
                let available = area_right.saturating_sub(start);
                let clipped: String = text.chars().take(available as usize).collect();
                (start, clipped)
            }
            MoveDirection::Rightward => {
                // Trail ends immediately before the gem; clip from the
                // front if the area can't fit the full length.
                let want_start = gem_x.saturating_sub(len);
                let start = want_start.max(area_left);
                let drop = (start - want_start) as usize;
                let clipped: String = text.chars().skip(drop).collect();
                (start, clipped)
            }
        };
        if render_text.is_empty() {
            continue;
        }
        let width = render_text.chars().count() as u16;
        let rect = Rect::new(x, y, width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(render_text, style))),
            rect,
        );
    }
}

fn draw_grand_gem(frame: &mut Frame, area: Rect, gem: &GemState) {
    // Each row is a list of (text, kind). `Kind::Gem` styles with the gem
    // color; `Kind::Shine` styles with the shine color. Splitting by kind
    // lets the two colors live on the same cell row.
    #[derive(Clone, Copy)]
    enum Kind {
        Gem,
        Shine,
    }

    let body: [&[(&str, Kind)]; 5] = [
        &[("    _________", Kind::Gem)],
        &[("   /_|_____|_\\", Kind::Gem)],
        &[("   '. \\   / .'", Kind::Gem)],
        &[("     '.\\ /.'", Kind::Gem)],
        &[("       '.'", Kind::Gem)],
    ];
    // Sparkle decorations layered on top when shining. Indices align with
    // the body rows; the extra row ABOVE the body is `shine_top`. Each
    // shining row carries 3 leading spaces so the shine's footprint is
    // symmetric around the body (the natural layout adds 3 chars on the
    // right but only replaces a leading space on the left); that way plain
    // `max_width` centering keeps the body columns stable across shining
    // and non-shining renders.
    let shine_top: &[(&str, Kind)] = &[("     .  `  '  `  .", Kind::Shine)];
    let shine_overlay: [&[(&str, Kind)]; 5] = [
        &[
            ("    `  ", Kind::Shine),
            ("_________", Kind::Gem),
            ("  `", Kind::Shine),
        ],
        &[
            ("   _  ", Kind::Shine),
            ("/_|_____|_\\", Kind::Gem),
            ("  _", Kind::Shine),
        ],
        // Body row 2 — unchanged content, just shifted right with the rest.
        &[("      '. \\   / .'", Kind::Gem)],
        &[
            ("     `  ", Kind::Shine),
            ("'.\\ /.'", Kind::Gem),
            ("  `", Kind::Shine),
        ],
        // Body row 4 — unchanged content, shifted with the rest.
        &[("          '.'", Kind::Gem)],
    ];

    let shining = gem.shining();
    let (rows, total_height): (Vec<&[(&str, Kind)]>, u16) = if shining {
        let mut v: Vec<&[(&str, Kind)]> = Vec::with_capacity(6);
        v.push(shine_top);
        for row in &shine_overlay {
            v.push(*row);
        }
        (v, 6)
    } else {
        (body.to_vec(), 5)
    };

    if area.height < total_height {
        gem.hit_area.set(None);
        return;
    }

    let row_widths: Vec<u16> = rows
        .iter()
        .map(|row| row.iter().map(|(s, _)| s.chars().count() as u16).sum())
        .collect();
    let max_width = row_widths.iter().copied().max().unwrap_or(0);
    if area.width < max_width {
        gem.hit_area.set(None);
        return;
    }

    let x_origin = area.x + (area.width.saturating_sub(max_width)) / 2;
    let y_origin = area.y + area.height.saturating_sub(total_height);
    let gem_style = Style::default().fg(gem.color());
    let shine_style = Style::default().fg(gem.shine_color());

    for (i, row) in rows.iter().enumerate() {
        let row_width: u16 = row.iter().map(|(s, _)| s.chars().count() as u16).sum();
        let row_rect = Rect::new(x_origin, y_origin + i as u16, row_width, 1);
        let spans: Vec<Span<'_>> = row
            .iter()
            .map(|(text, kind)| {
                let style = match kind {
                    Kind::Gem => gem_style,
                    Kind::Shine => shine_style,
                };
                Span::styled((*text).to_string(), style)
            })
            .collect();
        frame.render_widget(Paragraph::new(Line::from(spans)), row_rect);
    }

    gem.hit_area
        .set(Some(Rect::new(x_origin, y_origin, max_width, total_height)));
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

fn draw_delete_account_dialog(frame: &mut Frame, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(64, 12, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Delete Account ")
        .title_style(
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ERROR()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "This cannot be undone.",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Type your username to confirm:",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                state.draft().username.clone(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[3],
    );

    let typed = state.delete_account_dialog().input().lines().join("");
    let input_text = if typed.is_empty() {
        "username".to_string()
    } else if state.delete_account_dialog().pending() {
        typed.clone()
    } else {
        format!("{typed}█")
    };
    let input_style = if typed.is_empty() {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        Style::default().fg(theme::AMBER())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" › ", Style::default().fg(theme::AMBER_GLOW())),
            Span::styled(input_text, input_style),
        ])),
        layout[4],
    );

    if let Some(status) = state.delete_account_dialog().status() {
        let color = if state.delete_account_dialog().pending() {
            theme::AMBER()
        } else {
            theme::ERROR()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(status.to_string(), Style::default().fg(color)),
            ])),
            layout[5],
        );
    }

    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" delete  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[7]);
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

fn text_with_caret(text: &str, cursor_col: usize) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    chars.insert(cursor_col.min(chars.len()), '█');
    chars.into_iter().collect()
}

fn system_field_value(state: &SettingsModalState, row: Row, value: Option<String>) -> ValueSpan {
    if state.editing_system_row(row) {
        let typed = state.system_input().lines().join("");
        if typed.is_empty() {
            value_span("█", theme::AMBER())
        } else {
            value_span(
                text_with_caret(&typed, state.system_input().cursor().1),
                theme::AMBER(),
            )
        }
    } else {
        match value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => value_span(value.to_string(), theme::TEXT_BRIGHT()),
            None if row == Row::Langs => value_span("comma sep…", theme::TEXT_FAINT()),
            None => value_span("not set", theme::TEXT_FAINT()),
        }
    }
}

fn format_lang_tags(langs: &[String]) -> String {
    langs
        .iter()
        .map(|lang| format!("#{lang}"))
        .collect::<Vec<_>>()
        .join(" ")
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
    let selected = state.selected_row() == row
        && !state.editing_username()
        && state.editing_system_field().is_none()
        && !state.editing_bio();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_with_caret_uses_cursor_column() {
        assert_eq!(text_with_caret("abcd", 0), "█abcd");
        assert_eq!(text_with_caret("abcd", 2), "ab█cd");
        assert_eq!(text_with_caret("abcd", 4), "abcd█");
        assert_eq!(text_with_caret("abcd", 99), "abcd█");
    }
}
