use std::time::Duration;

use late_core::models::user::LandingPage;
use uuid::Uuid;

use crate::app::common::primitives::Screen;

/// How long a session goes without input from a person before a draining pod
/// moves it. The page and the music survive the move, so a wrong guess costs
/// a redraw and a scroll position.
pub(crate) const HANDOFF_IDLE: Duration = Duration::from_secs(5 * 60);

/// The env a CLI carries the resume point back in, on the new connection and
/// before its pty request. `late-cli/src/ssh.rs` spells it too.
pub(crate) const RESUME_ENV: &str = "LATE_RESUME";

/// The session's handoff bookkeeping. One handoff per session: once a CLI has
/// been asked to move, a session that stays (the dial failed, or the CLI
/// never acted on it) keeps the update banner.
#[derive(Debug, Default)]
pub(crate) struct HandoffState {
    pub(crate) sent: bool,
}

/// The page a moved session comes back to. Only pages are restored, never
/// what was open on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumePage {
    Clubhouse,
    Dashboard,
    Arcade,
    Games,
    Artboard,
    Profiles,
    Leaderboard,
    Calendars,
    Zen,
}

impl ResumePage {
    /// Where a session on `screen` comes back to: a page is itself, every
    /// other screen is the page it was reached from.
    pub(crate) fn from_screen(screen: Screen) -> Self {
        match screen {
            Screen::Clubhouse | Screen::Nightcap | Screen::City => Self::Clubhouse,
            Screen::Dashboard | Screen::DailyMatch | Screen::HouseTable | Screen::Scratchpad => {
                Self::Dashboard
            }
            Screen::Arcade => Self::Arcade,
            Screen::Games
            | Screen::Lateania
            | Screen::Rebels
            | Screen::Nethack
            | Screen::Dcss
            | Screen::Brogue
            | Screen::Dopewars
            | Screen::Bashquest
            | Screen::Codekeep
            | Screen::Zork
            | Screen::Usurper
            | Screen::GreenDragon
            | Screen::Darkroom
            | Screen::Realm => Self::Games,
            Screen::Artboard => Self::Artboard,
            Screen::Profiles => Self::Profiles,
            Screen::Leaderboard => Self::Leaderboard,
            Screen::Calendars => Self::Calendars,
            Screen::Zen => Self::Zen,
        }
    }

    pub(crate) fn screen(self) -> Screen {
        match self {
            Self::Clubhouse => Screen::Clubhouse,
            Self::Dashboard => Screen::Dashboard,
            Self::Arcade => Screen::Arcade,
            Self::Games => Screen::Games,
            Self::Artboard => Screen::Artboard,
            Self::Profiles => Screen::Profiles,
            Self::Leaderboard => Screen::Leaderboard,
            Self::Calendars => Screen::Calendars,
            Self::Zen => Screen::Zen,
        }
    }

    /// The landing a resumed session is built on. The three landings are
    /// themselves; any other page lands on Home and is switched to from
    /// there, like a digit pressed on the first frame.
    pub(crate) fn landing(self) -> LandingPage {
        match self {
            Self::Clubhouse => LandingPage::Clubhouse,
            Self::Zen => LandingPage::Zen,
            Self::Dashboard
            | Self::Arcade
            | Self::Games
            | Self::Artboard
            | Self::Profiles
            | Self::Leaderboard
            | Self::Calendars => LandingPage::Home,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Clubhouse => "clubhouse",
            Self::Dashboard => "dashboard",
            Self::Arcade => "arcade",
            Self::Games => "games",
            Self::Artboard => "artboard",
            Self::Profiles => "profiles",
            Self::Leaderboard => "leaderboard",
            Self::Calendars => "calendars",
            Self::Zen => "zen",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "clubhouse" => Some(Self::Clubhouse),
            "dashboard" => Some(Self::Dashboard),
            "arcade" => Some(Self::Arcade),
            "games" => Some(Self::Games),
            "artboard" => Some(Self::Artboard),
            "profiles" => Some(Self::Profiles),
            "leaderboard" => Some(Self::Leaderboard),
            "calendars" => Some(Self::Calendars),
            "zen" => Some(Self::Zen),
            _ => None,
        }
    }
}

/// Where a moved session was: its page and the chat room it had selected.
/// The room is a hint the new session checks against the user's own room
/// list, so a point naming a room the user cannot see lands on the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumePoint {
    pub(crate) page: ResumePage,
    pub(crate) room_id: Option<Uuid>,
}

const WIRE_VERSION: &str = "v1";

impl ResumePoint {
    /// The wire form: `v1:<page>` or `v1:<page>:<room id>`. The CLI carries
    /// it back verbatim and never reads it, so only servers agree on it.
    pub(crate) fn encode(&self) -> String {
        match self.room_id {
            Some(room_id) => format!("{WIRE_VERSION}:{}:{room_id}", self.page.key()),
            None => format!("{WIRE_VERSION}:{}", self.page.key()),
        }
    }

    /// Read what a CLI carried back. `None` for anything this server did not
    /// write, a later format or a hand-set env among them: that session
    /// starts as a fresh one.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split(':');
        let version = parts.next()?;
        let page = parts.next()?;
        let room = parts.next();
        if version != WIRE_VERSION || parts.next().is_some() {
            return None;
        }
        let page = ResumePage::from_key(page)?;
        let room_id = match room {
            None => None,
            Some(room) => Some(Uuid::parse_str(room).ok()?),
        };
        Some(Self { page, room_id })
    }
}

/// What decides whether a session moves this tick. The session's own key
/// presses are the idle clock; voice, a video, and an unsent draft are
/// things a person does without typing, so each holds the session in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandoffCheck {
    pub(crate) draining: bool,
    pub(crate) auto_restart: bool,
    pub(crate) idle: Duration,
    pub(crate) in_voice: bool,
    pub(crate) watching_video: bool,
    pub(crate) composer_draft: bool,
}

impl HandoffCheck {
    pub(crate) fn ready(&self) -> bool {
        self.draining
            && self.auto_restart
            && self.idle >= HANDOFF_IDLE
            && !self.in_voice
            && !self.watching_video
            && !self.composer_draft
    }
}
