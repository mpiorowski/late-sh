use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
#[derive(Debug, Clone)]
pub enum BannerKind {
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Banner {
    pub message: String,
    pub kind: BannerKind,
    pub created_at: Instant,
}

impl Banner {
    pub fn success(message: &str) -> Self {
        Self {
            message: message.to_string(),
            kind: BannerKind::Success,
            created_at: Instant::now(),
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            message: message.to_string(),
            kind: BannerKind::Error,
            created_at: Instant::now(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.created_at.elapsed().as_secs() < 5
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Arcade,
    Games,
    Rooms,
    Lateania,
    Rebels,
    Nethack,
    Artboard,
    Pinstar,
}

impl Screen {
    /// Tab cycles only the top-level pages. The three door games (Lateania,
    /// Rebels, Nethack) are reached through the Games hub, not the tab bar, so
    /// they are absent from the cycle; if one is somehow current, `next`/`prev`
    /// fall back to the hub that owns them.
    pub fn next(self) -> Self {
        match self {
            Screen::Dashboard => Screen::Arcade,
            Screen::Arcade => Screen::Games,
            Screen::Games => Screen::Rooms,
            Screen::Rooms => Screen::Artboard,
            Screen::Artboard => Screen::Pinstar,
            Screen::Pinstar => Screen::Dashboard,
            Screen::Lateania | Screen::Rebels | Screen::Nethack => Screen::Games,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Screen::Dashboard => Screen::Pinstar,
            Screen::Arcade => Screen::Dashboard,
            Screen::Games => Screen::Arcade,
            Screen::Rooms => Screen::Games,
            Screen::Artboard => Screen::Rooms,
            Screen::Pinstar => Screen::Artboard,
            Screen::Lateania | Screen::Rebels | Screen::Nethack => Screen::Games,
        }
    }
}

pub fn format_duration_mmss(duration: Duration) -> String {
    let secs = duration.as_secs();
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes}:{seconds:02}")
}

pub fn draw_tabs(frame: &mut Frame, area: Rect, current: Screen) {
    let label = match current {
        Screen::Dashboard => "Dashboard",
        Screen::Games => "Games",
        Screen::Lateania => "Lateania",
        Screen::Rebels => "Rebels",
        Screen::Nethack => "NetHack",
        Screen::Arcade => "Arcade",
        Screen::Rooms => "Tables",
        Screen::Artboard => "Artboard",
        Screen::Pinstar => "Directory",
    };

    let current_line = Paragraph::new(Line::from(vec![
        Span::styled("Current: ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            label,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(current_line, area);
}

pub fn draw_banner(frame: &mut Frame, area: Rect, banner: &Banner) {
    let (icon, color) = match banner.kind {
        BannerKind::Success => (" ✓ ", theme::SUCCESS()),
        BannerKind::Error => (" ✗ ", theme::ERROR()),
    };

    let content = Paragraph::new(Line::from(vec![
        Span::styled(icon, Style::default().fg(color)),
        Span::styled(&banner.message, Style::default().fg(color)),
    ]));

    frame.render_widget(content, area);
}

pub fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        let mins = diff.num_minutes();
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if diff.num_hours() < 24 {
        let hrs = diff.num_hours();
        format!("{} hr{} ago", hrs, if hrs == 1 { "" } else { "s" })
    } else if diff.num_days() < 7 {
        let days = diff.num_days();
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else {
        dt.format("%m-%d").to_string()
    }
}

/// Build a one-line action-hint footer: `key desc · key desc · …`.
///
/// Keys render in amber, descriptions dim, separators faint. This is the shared
/// recipe behind every bottom hint bar (the Directory footers, the Pinstar
/// browser) so the foot of each page reads the same.
pub(crate) fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme::TEXT_DIM());
    let sep_style = Style::default().fg(theme::TEXT_FAINT());

    let mut spans = Vec::with_capacity(hints.len() * 4 + 1);
    spans.push(Span::styled(" ", desc_style));
    for (idx, (key, desc)) in hints.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" · ", sep_style));
        }
        spans.push(Span::styled((*key).to_string(), key_style));
        spans.push(Span::styled(format!(" {desc}"), desc_style));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_next_cycles_top_level_screens() {
        assert_eq!(Screen::Dashboard.next(), Screen::Arcade);
        assert_eq!(Screen::Arcade.next(), Screen::Games);
        assert_eq!(Screen::Games.next(), Screen::Rooms);
        assert_eq!(Screen::Rooms.next(), Screen::Artboard);
        assert_eq!(Screen::Artboard.next(), Screen::Pinstar);
        assert_eq!(Screen::Pinstar.next(), Screen::Dashboard);
    }

    #[test]
    fn screen_prev_cycles_top_level_screens() {
        assert_eq!(Screen::Dashboard.prev(), Screen::Pinstar);
        assert_eq!(Screen::Arcade.prev(), Screen::Dashboard);
        assert_eq!(Screen::Games.prev(), Screen::Arcade);
        assert_eq!(Screen::Rooms.prev(), Screen::Games);
        assert_eq!(Screen::Artboard.prev(), Screen::Rooms);
        assert_eq!(Screen::Pinstar.prev(), Screen::Artboard);
    }

    #[test]
    fn door_games_are_outside_the_tab_cycle_and_fall_back_to_the_hub() {
        for door in [Screen::Lateania, Screen::Rebels, Screen::Nethack] {
            assert_eq!(door.next(), Screen::Games);
            assert_eq!(door.prev(), Screen::Games);
        }
    }

    #[test]
    fn format_duration_mmss_formats_minutes_and_seconds() {
        assert_eq!(format_duration_mmss(Duration::from_secs(0)), "0:00");
        assert_eq!(format_duration_mmss(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration_mmss(Duration::from_secs(3599)), "59:59");
    }

    #[test]
    fn banner_is_active_for_recent_messages() {
        let fresh = Banner::success("ok");
        assert!(fresh.is_active());

        let stale = Banner {
            message: "old".to_string(),
            kind: BannerKind::Error,
            created_at: Instant::now() - Duration::from_secs(6),
        };
        assert!(!stale.is_active());
    }
}
