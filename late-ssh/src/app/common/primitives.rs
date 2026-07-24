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
    /// Neutral news (a lost daily match, a draw): amber, not red — nothing
    /// went wrong, the user just needs to know.
    Info,
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

    pub fn info(message: &str) -> Self {
        Self {
            message: message.to_string(),
            kind: BannerKind::Info,
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
    Lateania,
    Rebels,
    Nethack,
    Dcss,
    Brogue,
    Dopewars,
    Usurper,
    GreenDragon,
    Artboard,
    Pinstar,
    Clubhouse,
    /// Full-screen daily-match board. Entered only from the Daily Games
    /// modal, absent from the Tab cycle; Esc returns to the modal.
    DailyMatch,
    /// Full-screen house table (poker/blackjack/asterion/tron). Entered only
    /// from the Lobby modal, absent from the Tab cycle; Esc returns to the
    /// modal.
    HouseTable,
}

impl Screen {
    /// Tab cycles the top-level pages, Clubhouse (`0`, the landing screen)
    /// through Directory (`5`). The door games (Lateania, Rebels, Nethack,
    /// Green Dragon) are reached through the Games hub, not the tab bar, so
    /// they are absent from the cycle; if one is somehow current,
    /// `next`/`prev` fall back to the hub that owns them.
    pub fn next(self) -> Self {
        match self {
            Screen::Clubhouse => Screen::Dashboard,
            Screen::Dashboard => Screen::Arcade,
            Screen::Arcade => Screen::Games,
            Screen::Games => Screen::Artboard,
            Screen::Artboard => Screen::Pinstar,
            Screen::Pinstar => Screen::Clubhouse,
            Screen::Lateania
            | Screen::Rebels
            | Screen::Nethack
            | Screen::Dcss
            | Screen::Brogue
            | Screen::Dopewars
            | Screen::Usurper
            | Screen::GreenDragon => Screen::Games,
            Screen::DailyMatch => Screen::Dashboard,
            Screen::HouseTable => Screen::Dashboard,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Screen::Clubhouse => Screen::Pinstar,
            Screen::Dashboard => Screen::Clubhouse,
            Screen::Arcade => Screen::Dashboard,
            Screen::Games => Screen::Arcade,
            Screen::Artboard => Screen::Games,
            Screen::Pinstar => Screen::Artboard,
            Screen::Lateania
            | Screen::Rebels
            | Screen::Nethack
            | Screen::Dcss
            | Screen::Brogue
            | Screen::Dopewars
            | Screen::Usurper
            | Screen::GreenDragon => Screen::Games,
            Screen::DailyMatch => Screen::Dashboard,
            Screen::HouseTable => Screen::Dashboard,
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
        Screen::Dcss => "DCSS",
        Screen::Brogue => "Brogue",
        Screen::Dopewars => "dopewars",
        Screen::Usurper => "Usurper",
        Screen::GreenDragon => "Green Dragon",
        Screen::Arcade => "Arcade",
        Screen::Artboard => "Artboard",
        Screen::Pinstar => "Directory",
        Screen::Clubhouse => "Clubhouse",
        Screen::DailyMatch => "Daily Match",
        Screen::HouseTable => "House Table",
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
        BannerKind::Info => (" • ", theme::AMBER()),
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

/// Compact relative stamp for tight rows: `now`, `5m`, `3h`, `2d`, `06-12`.
pub fn format_relative_time_short(dt: chrono::DateTime<chrono::Utc>) -> String {
    let diff = chrono::Utc::now().signed_duration_since(dt);
    if diff.num_seconds() < 60 {
        "now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d", diff.num_days())
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
