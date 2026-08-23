use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

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
    Bashquest,
    Codekeep,
    Usurper,
    GreenDragon,
    Darkroom,
    Artboard,
    Profiles,
    Leaderboard,
    Clubhouse,
    /// Full-screen daily-match board. Entered only from the Daily Games
    /// modal, absent from the Tab cycle; Esc returns to the modal.
    DailyMatch,
    /// Full-screen house table (poker/blackjack/asterion/tron). Entered only
    /// from the Lobby modal, absent from the Tab cycle; Esc returns to the
    /// modal.
    HouseTable,
    /// Paired live coding scratchpad. Entered only once both users have run
    /// `/pair @other`, absent from the Tab cycle; Esc leaves the pairing.
    Scratchpad,
}

impl Screen {
    /// Tab cycles the top-level pages, Clubhouse (`0`, the landing screen)
    /// through Leaderboards (`6`). The door games (Lateania, Rebels, Nethack,
    /// Green Dragon) are reached through the Games hub, not the tab bar, so
    /// they are absent from the cycle; if one is somehow current,
    /// `next`/`prev` fall back to the hub that owns them.
    pub fn next(self) -> Self {
        match self {
            Screen::Clubhouse => Screen::Dashboard,
            Screen::Dashboard => Screen::Arcade,
            Screen::Arcade => Screen::Games,
            Screen::Games => Screen::Artboard,
            Screen::Artboard => Screen::Profiles,
            Screen::Profiles => Screen::Leaderboard,
            Screen::Leaderboard => Screen::Clubhouse,
            Screen::Lateania
            | Screen::Rebels
            | Screen::Nethack
            | Screen::Dcss
            | Screen::Brogue
            | Screen::Dopewars
            | Screen::Bashquest
            | Screen::Codekeep
            | Screen::Usurper
            | Screen::GreenDragon
            | Screen::Darkroom => Screen::Games,
            Screen::DailyMatch => Screen::Dashboard,
            Screen::HouseTable => Screen::Dashboard,
            Screen::Scratchpad => Screen::Dashboard,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Screen::Clubhouse => Screen::Leaderboard,
            Screen::Dashboard => Screen::Clubhouse,
            Screen::Arcade => Screen::Dashboard,
            Screen::Games => Screen::Arcade,
            Screen::Artboard => Screen::Games,
            Screen::Profiles => Screen::Artboard,
            Screen::Leaderboard => Screen::Profiles,
            Screen::Lateania
            | Screen::Rebels
            | Screen::Nethack
            | Screen::Dcss
            | Screen::Brogue
            | Screen::Dopewars
            | Screen::Bashquest
            | Screen::Codekeep
            | Screen::Usurper
            | Screen::GreenDragon
            | Screen::Darkroom => Screen::Games,
            Screen::DailyMatch => Screen::Dashboard,
            Screen::HouseTable => Screen::Dashboard,
            Screen::Scratchpad => Screen::Dashboard,
        }
    }
}

/// One row with `left` at the start and `right` flushed to the right edge, for
/// header rows that pair live status with the keys that act on it. The right
/// side is a hint, so a row too tight to hold both keeps the left side and
/// drops the hint rather than wrapping or colliding.
pub fn row_with_hint(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    let span_width =
        |spans: &[Span<'static>]| -> usize { spans.iter().map(|s| s.content.width()).sum() };
    let left_width = span_width(&left);
    let right_width = span_width(&right);
    if right_width == 0 || left_width + right_width + 2 > width {
        return Line::from(left);
    }
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(width - left_width - right_width)));
    spans.extend(right);
    Line::from(spans)
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
        Screen::Bashquest => "BashQuest",
        Screen::Codekeep => "CodeKeep",
        Screen::Usurper => "Usurper",
        Screen::GreenDragon => "Green Dragon",
        Screen::Darkroom => crate::app::door::darkroom::data::TITLE,
        Screen::Arcade => "Arcade",
        Screen::Artboard => "Artboard",
        Screen::Profiles => "Profiles",
        Screen::Leaderboard => "Leaderboards",
        Screen::Clubhouse => "Clubhouse",
        Screen::DailyMatch => "Daily Match",
        Screen::HouseTable => "House Table",
        Screen::Scratchpad => "Scratchpad",
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

/// The one "your terminal is too small" line, for every screen that has a
/// minimum size. Always names what needs the room, the size it needs, and the
/// size you currently have: a bare "too small" leaves people resizing blind
/// with no idea how far they have to go (user feedback). It says "space", not
/// "terminal": callers pass their constrained inner area, which is smaller
/// than the terminal by whatever chrome surrounds it.
pub fn too_small_text(what: &str, min_width: u16, min_height: u16, area: Rect) -> String {
    format!(
        "{what} needs at least {min_width}×{min_height}, this space is {}×{}",
        area.width, area.height
    )
}

/// Render [`too_small_text`] centred in `area`, wrapped so the line survives a
/// terminal narrower than the message itself.
pub fn draw_too_small(frame: &mut Frame, area: Rect, what: &str, min_width: u16, min_height: u16) {
    frame.render_widget(
        Paragraph::new(too_small_text(what, min_width, min_height, area))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme::ERROR())),
        area,
    );
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
/// recipe behind every bottom hint bar (the Profiles footer, the Artboard
/// view bar) so the foot of each page reads the same.
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

/// Group digits with commas: `10000` → `"10,000"`. The shared formatter for
/// every chip, score, and progress figure, so numbers read the same on all
/// surfaces.
pub(crate) fn thousands(value: i64) -> String {
    let raw = value.to_string();
    let (sign, digits) = raw
        .strip_prefix('-')
        .map_or(("", raw.as_str()), |rest| ("-", rest));
    let mut out = String::with_capacity(sign.len() + digits.len() + digits.len() / 3);
    out.push_str(sign);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
