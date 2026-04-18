use late_core::models::profile::Profile;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::ai::ghost::{GRAYBEARD_CHAT_INTERVAL, GRAYBEARD_MENTION_COOLDOWN};
use crate::app::common::theme;
use late_core::models::leaderboard::BadgeTier;

pub struct ProfileRenderInput<'a> {
    pub profile: &'a Profile,
    pub editing_username: bool,
    pub username_composer: &'a str,
    pub ai_model: &'a str,
    pub theme_id: &'a str,
    pub scroll_offset: u16,
    pub current_streak: u32,
    pub chip_balance: i64,
    pub tetris_best: i32,
    pub twenty_forty_eight_best: i32,
    pub cursor_visible: bool,
    pub notify_kinds: &'a [String],
    pub notify_bell: bool,
    pub notify_cooldown_mins: i32,
    pub settings_row: usize,
}

pub fn draw_profile(frame: &mut Frame, area: Rect, view: &ProfileRenderInput<'_>) {
    let lines = build_lines(view, area.width);
    let paragraph = Paragraph::new(lines).scroll((view.scroll_offset, 0));
    frame.render_widget(paragraph, area);
}

fn build_lines<'a>(view: &ProfileRenderInput<'a>, width: u16) -> Vec<Line<'a>> {
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut lines: Vec<Line<'a>> = Vec::with_capacity(64);

    // ── Your Settings ──
    lines.push(Line::from(""));
    lines.push(section_heading("Your Settings"));

    // Username box
    lines.push(Line::from(""));

    let box_w = (width.saturating_sub(6) as usize).min(42);

    let username_border_color = if view.editing_username {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let border_style = Style::default().fg(username_border_color);

    // Top border with inline title (like chat composer)
    let title = if view.editing_username {
        " Username (Enter save, Esc cancel) "
    } else {
        " Username (i edit) "
    };
    let title_len = title.len();
    let right_pad = box_w.saturating_sub(title_len + 1);
    lines.push(Line::from(vec![
        Span::styled("  \u{250c}\u{2500}", border_style),
        Span::styled(title.to_string(), border_style),
        Span::styled(
            format!("{}\u{2510}", "\u{2500}".repeat(right_pad)),
            border_style,
        ),
    ]));

    // Content line
    let cursor = if view.cursor_visible { "\u{2588}" } else { " " };
    let content_spans = if view.editing_username {
        if view.username_composer.is_empty() {
            let placeholder_first = if view.cursor_visible {
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            vec![
                Span::styled("  \u{2502} ", border_style),
                Span::styled("e", placeholder_first),
                Span::styled("nter username", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    format!("{}\u{2502}", " ".repeat(box_w.saturating_sub(15))),
                    border_style,
                ),
            ]
        } else {
            let composer_len = view.username_composer.len();
            let padding = " ".repeat(box_w.saturating_sub(composer_len + 2));
            vec![
                Span::styled("  \u{2502} ", border_style),
                Span::styled(view.username_composer, Style::default().fg(theme::TEXT())),
                Span::styled(cursor.to_string(), Style::default().fg(theme::AMBER_GLOW())),
                Span::styled(format!("{padding}\u{2502}"), border_style),
            ]
        }
    } else if view.profile.username.is_empty() {
        let padding = " ".repeat(box_w.saturating_sub(8));
        vec![
            Span::styled("  \u{2502} ", border_style),
            Span::styled("not set", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(format!("{padding}\u{2502}"), border_style),
        ]
    } else {
        let name_len = view.profile.username.len();
        let padding = " ".repeat(box_w.saturating_sub(name_len + 1));
        vec![
            Span::styled("  \u{2502} ", border_style),
            Span::styled(
                view.profile.username.as_str(),
                Style::default().fg(theme::TEXT()),
            ),
            Span::styled(format!("{padding}\u{2502}"), border_style),
        ]
    };
    lines.push(Line::from(content_spans));

    let bottom_border = format!("  \u{2514}{}\u{2518}", "\u{2500}".repeat(box_w));
    lines.push(Line::from(Span::styled(bottom_border, border_style)));

    let nav_style = Style::default().fg(theme::TEXT_FAINT());
    let selected_label = Style::default().fg(theme::TEXT());
    let label_pad: usize = 33;

    let theme_row_style = if view.settings_row == 0 {
        selected_label
    } else {
        dim
    };
    let theme_marker = if view.settings_row == 0 {
        "\u{203a}"
    } else {
        " "
    };
    let theme_label_text = " Theme";
    let theme_pad = " ".repeat(label_pad.saturating_sub(theme_label_text.len() + 1));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  Themes: {}.", theme::help_text()),
        Style::default().fg(theme::TEXT_MUTED()),
    )));
    lines.push(Line::from(vec![
        Span::styled(format!(" {theme_marker}"), nav_style),
        Span::styled(theme_label_text, theme_row_style),
        Span::styled(theme_pad, dim),
        Span::styled("\u{25c0} ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            theme::label_for_id(view.theme_id),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(" \u{25b6}", Style::default().fg(theme::TEXT_DIM())),
    ]));

    // Black background toggle row
    let background_color_selected = view.settings_row == 1;
    let background_color_row_style = if background_color_selected {
        selected_label
    } else {
        dim
    };
    let background_color_marker = if background_color_selected {
        "\u{203a}"
    } else {
        " "
    };
    let bb_label = " Enable Background Color";
    let bb_pad = " ".repeat(label_pad.saturating_sub(bb_label.len() + 1));
    let checkbox = if view.profile.enable_background_color {
        "[x]"
    } else {
        "[ ]"
    };
    let checkbox_style = if view.profile.enable_background_color {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {background_color_marker}"), nav_style),
        Span::styled(bb_label, background_color_row_style),
        Span::styled(bb_pad, dim),
        Span::styled(checkbox, checkbox_style),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Notification settings",
        Style::default().fg(theme::TEXT_MUTED()),
    )));

    // Kind checkboxes. Keep this list in sync with ProfileState::NOTIFY_KINDS.
    let kinds: [(&str, &str); 3] = [
        ("dms", "Direct messages"),
        ("mentions", "@mentions"),
        ("game_events", "Game events"),
    ];

    for (row_idx, (kind, label)) in kinds.iter().enumerate() {
        let enabled = view.notify_kinds.iter().any(|k| k == *kind);
        let settings_row = row_idx + 2; // rows 0=theme, 1=background_color, 2..=notify
        lines.push(checkbox_row(
            label,
            enabled,
            view.settings_row == settings_row,
            label_pad,
        ));
    }

    // Terminal bell notification checkbox.
    let bell_row = kinds.len() + 2; // 0=theme, 1=background_color, 2..4=notify, 5=bell
    lines.push(checkbox_row(
        "Bell",
        view.notify_bell,
        view.settings_row == bell_row,
        label_pad,
    ));

    // Cooldown row (last).
    let cooldown_row = kinds.len() + 3; // 0=theme, 1=background_color, 2..4=notify, 5=bell, 6=cooldown
    let cooldown_row_style = if view.settings_row == cooldown_row {
        selected_label
    } else {
        dim
    };
    let cooldown_marker = if view.settings_row == cooldown_row {
        "\u{203a}"
    } else {
        " "
    };
    let cooldown_label_text = " Cooldown (mins)";
    let cooldown_pad = " ".repeat(label_pad.saturating_sub(cooldown_label_text.len() + 1));
    let cooldown_val = if view.notify_cooldown_mins == 0 {
        "Off".to_string()
    } else {
        format!("{}", view.notify_cooldown_mins)
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {cooldown_marker}"), nav_style),
        Span::styled(cooldown_label_text, cooldown_row_style),
        Span::styled(cooldown_pad, dim),
        Span::styled("\u{25c0} ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(cooldown_val, Style::default().fg(theme::AMBER())),
        Span::styled(" \u{25b6}", Style::default().fg(theme::TEXT_DIM())),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Up/Down select a setting. Left/Right change it. Space/Enter toggles.",
        dim,
    )));

    // ── Notifications ──
    lines.push(Line::from(""));
    lines.push(section_heading("Notifications"));

    lines.push(Line::from(Span::styled(
        "  Desktop notifications delivered to your terminal via",
        dim,
    )));
    lines.push(Line::from(vec![
        Span::styled("  ", dim),
        Span::styled("OSC 777", Style::default().fg(theme::TEXT())),
        Span::styled(" (kitty, Ghostty, rxvt-unicode, foot,", dim),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  wezterm, konsole) and ", dim),
        Span::styled("OSC 9", Style::default().fg(theme::TEXT())),
        Span::styled(" (iTerm2). Unsupported", dim),
    ]));
    lines.push(Line::from(Span::styled(
        "  terminals silently ignore both.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  tmux is not supported — run directly in a terminal.",
        dim,
    )));

    // ── Your Stats ──
    lines.push(Line::from(""));
    lines.push(section_heading("Your Stats"));

    let streak = view.current_streak;
    let badge = BadgeTier::from_streak(streak);

    if streak == 0 {
        lines.push(Line::from(vec![
            Span::styled("  Daily Streak: ", dim),
            Span::styled("none", Style::default().fg(theme::TEXT_FAINT())),
        ]));
    } else {
        let badge_color = match badge {
            Some(BadgeTier::Gold) => theme::BADGE_GOLD(),
            Some(BadgeTier::Silver) => theme::BADGE_SILVER(),
            Some(BadgeTier::Bronze) => theme::BADGE_BRONZE(),
            None => theme::TEXT(),
        };
        let badge_label = badge.map(|b| format!(" {}", b.label())).unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled("  Daily Streak: ", dim),
            Span::styled(
                format!("{streak} day{}", if streak == 1 { "" } else { "s" }),
                Style::default()
                    .fg(badge_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(badge_label, Style::default().fg(badge_color)),
        ]));

        let next_tier = match badge {
            None => Some(("Bronze", 3)),
            Some(BadgeTier::Bronze) => Some(("Silver", 7)),
            Some(BadgeTier::Silver) => Some(("Gold", 14)),
            Some(BadgeTier::Gold) => None,
        };
        if let Some((tier_name, target)) = next_tier {
            let remaining = target - streak;
            lines.push(Line::from(Span::styled(
                format!(
                    "  {remaining} more day{} to {tier_name}",
                    if remaining == 1 { "" } else { "s" }
                ),
                dim,
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  Max tier reached!",
                Style::default()
                    .fg(theme::BADGE_GOLD())
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Late Chips: ", dim),
        Span::styled(
            format!("{}", view.chip_balance),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if view.tetris_best > 0 {
        lines.push(Line::from(vec![
            Span::styled("  Tetris:     ", dim),
            Span::styled(
                format!("{}", view.tetris_best),
                Style::default()
                    .fg(theme::TEXT())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if view.twenty_forty_eight_best > 0 {
        lines.push(Line::from(vec![
            Span::styled("  2048:       ", dim),
            Span::styled(
                format!("{}", view.twenty_forty_eight_best),
                Style::default()
                    .fg(theme::TEXT())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    // ── @bot ──
    lines.push(Line::from(""));
    lines.push(section_heading("@bot"));

    lines.push(Line::from(Span::styled(
        "  Mention @bot in any chat message to get",
        dim,
    )));
    lines.push(Line::from(vec![
        Span::styled("  AI-powered help, powered by ", dim),
        Span::styled(view.ai_model, Style::default().fg(theme::TEXT())),
        Span::styled(".", dim),
    ]));
    lines.push(Line::from(Span::styled(
        "  Ask about late.sh features, architecture,",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  how things work, or general dev questions.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  30s cooldown per user to prevent abuse.",
        dim,
    )));

    // ── @graybeard ──
    lines.push(Line::from(""));
    lines.push(section_heading("@graybeard"));

    let interval_min = GRAYBEARD_CHAT_INTERVAL.as_secs() / 60;
    let mention_cooldown_sec = GRAYBEARD_MENTION_COOLDOWN.as_secs();
    lines.push(Line::from(Span::styled(
        "  One ghost user haunts #general — a burned-out dev who",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        format!("  moans about the good old days (every ~{interval_min}min)."),
        dim,
    )));
    lines.push(Line::from(Span::styled(
        format!("  Replies when @mentioned ({mention_cooldown_sec}s cooldown)."),
        dim,
    )));

    // ── Chat Colors ──
    lines.push(Line::from(""));
    lines.push(section_heading("Chat Colors"));

    lines.push(Line::from(Span::styled(
        "  How usernames appear in chat:",
        dim,
    )));

    // Color legend with actual colored dots
    lines.push(Line::from(vec![
        Span::styled("    ", dim),
        Span::styled(
            "\u{25cf}",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" your username          ", dim),
        Span::styled(
            "amber bold",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    ", dim),
        Span::styled("\u{25cf}", Style::default().fg(theme::CHAT_AUTHOR())),
        Span::styled(" other users            ", dim),
        Span::styled("blue-grey", Style::default().fg(theme::CHAT_AUTHOR())),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    ", dim),
        Span::styled("\u{25cf}", Style::default().fg(theme::BOT())),
        Span::styled(" @bot / @graybeard      ", dim),
        Span::styled("muted purple", Style::default().fg(theme::BOT())),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    ", dim),
        Span::styled(
            "\u{25cf}",
            Style::default()
                .fg(theme::MENTION())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" @mentions              ", dim),
        Span::styled(
            "yellow bold",
            Style::default()
                .fg(theme::MENTION())
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(""));

    lines
}

fn checkbox_row(label: &str, enabled: bool, selected: bool, label_pad: usize) -> Line<'static> {
    let nav_style = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let row_style = if selected {
        Style::default().fg(theme::TEXT())
    } else {
        dim
    };
    let marker = if selected { "\u{203a}" } else { " " };
    let label_text = format!(" {label}");
    let pad = " ".repeat(label_pad.saturating_sub(label_text.len() + 1));
    let checkbox = if enabled { "[x]" } else { "[ ]" };
    let checkbox_style = if enabled {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::styled(format!(" {marker}"), nav_style),
        Span::styled(label_text, row_style),
        Span::styled(pad, dim),
        Span::styled(checkbox, checkbox_style),
    ])
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let bold_cyan = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  \u{2500}\u{2500} ", dim),
        Span::styled(title.to_string(), bold_cyan),
        Span::styled(" \u{2500}\u{2500}", dim),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_lines_contains_expected_sections() {
        let profile = Profile::default();
        let kinds: Vec<String> = Vec::new();
        let view = ProfileRenderInput {
            profile: &profile,
            editing_username: false,
            username_composer: "",
            ai_model: "gemini-3-flash",
            theme_id: "late",
            scroll_offset: 0,
            current_streak: 5,
            chip_balance: 750,
            tetris_best: 1200,
            twenty_forty_eight_best: 8192,
            cursor_visible: false,
            notify_kinds: &kinds,
            notify_bell: false,
            notify_cooldown_mins: 0,
            settings_row: 0,
        };
        let lines = build_lines(&view, 80);
        let text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("Username"));
        assert!(text.contains("@graybeard"));
        assert!(text.contains("Your Stats"));
        assert!(text.contains("5 days"));
        assert!(text.contains("@bot"));
        assert!(text.contains("Chat Colors"));
        assert!(text.contains("gemini-3-flash"));
    }
}
