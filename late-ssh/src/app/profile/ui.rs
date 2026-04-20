use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use late_core::models::leaderboard::BadgeTier;
use late_core::models::profile::Profile;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    ai::ghost::{GRAYBEARD_CHAT_INTERVAL, GRAYBEARD_MENTION_COOLDOWN},
    common::{composer::build_composer_rows, theme},
    settings_modal::{self, data::country_label},
};

pub struct ProfileRenderInput<'a> {
    pub profile: &'a Profile,
    pub ai_model: &'a str,
    pub scroll_offset: u16,
    pub current_streak: u32,
    pub chip_balance: i64,
    pub tetris_best: i32,
    pub twenty_forty_eight_best: i32,
}

pub fn draw_profile(frame: &mut Frame, area: Rect, view: &ProfileRenderInput<'_>) {
    let lines = build_lines(view);
    frame.render_widget(Paragraph::new(lines).scroll((view.scroll_offset, 0)), area);
}

fn build_lines<'a>(view: &ProfileRenderInput<'a>) -> Vec<Line<'a>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut lines = Vec::new();

    lines.push(Line::from(""));
    lines.push(section_heading("Profile"));
    lines.push(Line::from(vec![
        Span::styled("  Username: ", dim),
        Span::styled(
            if view.profile.username.is_empty() {
                "not set"
            } else {
                view.profile.username.as_str()
            },
            Style::default().fg(theme::TEXT()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Country:  ", dim),
        Span::styled(
            country_label(view.profile.country.as_deref()),
            Style::default().fg(theme::TEXT()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Timezone: ", dim),
        Span::styled(
            view.profile.timezone.as_deref().unwrap_or("Not set"),
            Style::default().fg(theme::TEXT()),
        ),
    ]));
    if let Some(current_time) = timezone_current_time(Utc::now(), view.profile.timezone.as_deref())
    {
        lines.push(Line::from(vec![
            Span::styled("  Current time: ", dim),
            Span::styled(current_time, Style::default().fg(theme::TEXT())),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Bio",
        Style::default().fg(theme::TEXT_MUTED()),
    )));
    if view.profile.bio.trim().is_empty() {
        lines.push(Line::from(Span::styled("  Not set", dim)));
    } else {
        let wrap_width = settings_modal::ui::bio_text_width(settings_modal::ui::MODAL_WIDTH);
        for row in build_composer_rows(&view.profile.bio, wrap_width) {
            lines.push(Line::from(Span::styled(
                format!("  {}", row.text),
                Style::default().fg(theme::TEXT()),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press Enter or e to edit profile settings",
        Style::default().fg(theme::AMBER_DIM()),
    )));

    lines.push(Line::from(""));
    lines.push(section_heading("Notifications"));
    lines.push(Line::from(Span::styled(
        "  Terminal notifications run through OSC 777 / OSC 9.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  Best support today: kitty, Ghostty, rxvt-unicode, foot, wezterm, konsole, and iTerm2.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  tmux is not supported here, so notification escape sequences can get mangled or dropped.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  They can fire for DMs, mentions, and game events.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  Bell and cooldown decide how loud and how often they show up.",
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "  Configure notification kinds, bell, and cooldown in the profile modal.",
        dim,
    )));

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
        lines.push(Line::from(vec![
            Span::styled("  Daily Streak: ", dim),
            Span::styled(
                format!("{streak} day{}", if streak == 1 { "" } else { "s" }),
                Style::default()
                    .fg(badge_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("  Late Chips:   ", dim),
        Span::styled(
            format!("{}", view.chip_balance),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if view.tetris_best > 0 {
        lines.push(Line::from(vec![
            Span::styled("  Tetris:       ", dim),
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
            Span::styled("  2048:         ", dim),
            Span::styled(
                format!("{}", view.twenty_forty_eight_best),
                Style::default()
                    .fg(theme::TEXT())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(section_heading("@bot"));
    lines.push(Line::from(vec![
        Span::styled("  Powered by ", dim),
        Span::styled(view.ai_model, Style::default().fg(theme::TEXT())),
        Span::styled(" with a 30s cooldown.", dim),
    ]));

    lines.push(Line::from(""));
    lines.push(section_heading("@graybeard"));
    let interval_min = GRAYBEARD_CHAT_INTERVAL.as_secs() / 60;
    let mention_cooldown_sec = GRAYBEARD_MENTION_COOLDOWN.as_secs();
    lines.push(Line::from(Span::styled(
        format!("  Lurks in #general every ~{interval_min}min."),
        dim,
    )));
    lines.push(Line::from(Span::styled(
        format!("  Replies on mention with a {mention_cooldown_sec}s cooldown."),
        dim,
    )));

    lines.push(Line::from(""));
    lines
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

pub(crate) fn timezone_current_time(now: DateTime<Utc>, timezone: Option<&str>) -> Option<String> {
    let timezone = timezone?.trim();
    if timezone.is_empty() {
        return None;
    }
    let tz: Tz = timezone.parse().ok()?;
    Some(now.with_timezone(&tz).format("%a %H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn build_lines_contains_profile_summary_and_edit_hint() {
        let profile = Profile::default();
        let view = ProfileRenderInput {
            profile: &profile,
            ai_model: "gemini-3-flash",
            scroll_offset: 0,
            current_streak: 5,
            chip_balance: 750,
            tetris_best: 1200,
            twenty_forty_eight_best: 8192,
        };
        let lines = build_lines(&view);
        let text = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("Profile"));
        assert!(text.contains("Press Enter or e to edit profile settings"));
        assert!(text.contains("Timezone"));
        assert!(text.contains("@graybeard"));
        assert!(text.contains("gemini-3-flash"));
    }

    #[test]
    fn timezone_current_time_formats_valid_timezone() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 19, 12, 30, 0)
            .single()
            .unwrap();
        assert_eq!(
            timezone_current_time(now, Some("Europe/Warsaw")).as_deref(),
            Some("Sun 14:30")
        );
    }

    #[test]
    fn timezone_current_time_ignores_invalid_timezone() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 19, 12, 30, 0)
            .single()
            .unwrap();
        assert_eq!(timezone_current_time(now, Some("not/a-timezone")), None);
    }
}
