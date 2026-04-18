use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::app::vote::svc::Genre;

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
    Chat,
    Profile,
    Games,
}

impl Screen {
    pub fn next(self) -> Self {
        match self {
            Screen::Dashboard => Screen::Chat,
            Screen::Chat => Screen::Games,
            Screen::Games => Screen::Profile,
            Screen::Profile => Screen::Dashboard,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Screen::Dashboard => Screen::Profile,
            Screen::Chat => Screen::Dashboard,
            Screen::Games => Screen::Chat,
            Screen::Profile => Screen::Games,
        }
    }
}

pub fn genre_label(genre: Genre) -> &'static str {
    match genre {
        Genre::Lofi => "Lofi",
        Genre::Classic => "Classic",
        Genre::Ambient => "Ambient",
        Genre::Jazz => "Jazz",
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
        Screen::Chat => "Chat",
        Screen::Profile => "Profile",
        Screen::Games => "Games",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_next_cycles_all_screens() {
        assert_eq!(Screen::Dashboard.next(), Screen::Chat);
        assert_eq!(Screen::Chat.next(), Screen::Games);
        assert_eq!(Screen::Games.next(), Screen::Profile);
        assert_eq!(Screen::Profile.next(), Screen::Dashboard);
    }

    #[test]
    fn screen_prev_cycles_all_screens() {
        assert_eq!(Screen::Dashboard.prev(), Screen::Profile);
        assert_eq!(Screen::Chat.prev(), Screen::Dashboard);
        assert_eq!(Screen::Games.prev(), Screen::Chat);
        assert_eq!(Screen::Profile.prev(), Screen::Games);
    }

    #[test]
    fn genre_label_maps_variants() {
        assert_eq!(genre_label(Genre::Lofi), "Lofi");
        assert_eq!(genre_label(Genre::Classic), "Classic");
        assert_eq!(genre_label(Genre::Ambient), "Ambient");
        assert_eq!(genre_label(Genre::Jazz), "Jazz");
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
