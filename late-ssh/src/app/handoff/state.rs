use std::time::Duration;

use late_core::models::user::LandingPage;
use uuid::Uuid;

use crate::app::common::primitives::Page;

/// The env a CLI carries the resume point back in, on the new connection and
/// before its pty request. `late-cli/src/ssh.rs` spells it too.
pub(crate) const RESUME_ENV: &str = "LATE_RESUME";

/// The session's handoff bookkeeping. One handoff per session: once a CLI has
/// been asked to move, a session that stays (the dial failed, or the CLI
/// never acted on it) keeps the update banner.
#[derive(Debug)]
pub(crate) struct HandoffState {
    /// How long the session goes without input from a person before a
    /// draining pod moves it, from the profile (`Config::handoff_idle`).
    pub(super) idle_needed: Duration,
    pub(super) sent: bool,
}

impl HandoffState {
    pub(crate) fn new(idle_needed: Duration) -> Self {
        Self {
            idle_needed,
            sent: false,
        }
    }
}

/// The wire spelling of a page in a resume point.
fn page_key(page: Page) -> &'static str {
    match page {
        Page::Clubhouse => "clubhouse",
        Page::Dashboard => "dashboard",
        Page::Arcade => "arcade",
        Page::Games => "games",
        Page::Artboard => "artboard",
        Page::Profiles => "profiles",
        Page::Leaderboard => "leaderboard",
        Page::Calendars => "calendars",
        Page::Zen => "zen",
    }
}

fn page_from_key(key: &str) -> Option<Page> {
    match key {
        "clubhouse" => Some(Page::Clubhouse),
        "dashboard" => Some(Page::Dashboard),
        "arcade" => Some(Page::Arcade),
        "games" => Some(Page::Games),
        "artboard" => Some(Page::Artboard),
        "profiles" => Some(Page::Profiles),
        "leaderboard" => Some(Page::Leaderboard),
        "calendars" => Some(Page::Calendars),
        "zen" => Some(Page::Zen),
        _ => None,
    }
}

/// Where a moved session was: the page of the screen it was on
/// (`Screen::page`) and the chat room it had selected. Only the page comes
/// back, never what was open on it. The room is a hint the new session
/// checks against the user's own room list, so a point naming a room the
/// user cannot see lands on the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumePoint {
    pub(crate) page: Page,
    pub(crate) room_id: Option<Uuid>,
}

const WIRE_VERSION: &str = "v1";

impl ResumePoint {
    /// The wire form: `v1:<page>` or `v1:<page>:<room id>`. The CLI carries
    /// it back verbatim and never reads it, so only servers agree on it.
    pub(crate) fn encode(&self) -> String {
        match self.room_id {
            Some(room_id) => format!("{WIRE_VERSION}:{}:{room_id}", page_key(self.page)),
            None => format!("{WIRE_VERSION}:{}", page_key(self.page)),
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
        let page = page_from_key(page)?;
        let room_id = match room {
            None => None,
            Some(room) => Some(Uuid::parse_str(room).ok()?),
        };
        Some(Self { page, room_id })
    }

    /// The landing a resumed session is built on. The three landings are
    /// themselves; any other page lands on Home and is switched to from
    /// there, like a digit pressed on the first frame.
    pub(crate) fn landing(&self) -> LandingPage {
        match self.page {
            Page::Clubhouse => LandingPage::Clubhouse,
            Page::Zen => LandingPage::Zen,
            Page::Dashboard
            | Page::Arcade
            | Page::Games
            | Page::Artboard
            | Page::Profiles
            | Page::Leaderboard
            | Page::Calendars => LandingPage::Home,
        }
    }
}

/// What decides whether a session on a draining pod moves this tick. The
/// session's own key presses are the idle clock; voice, a video, and an
/// unsent draft are things a person does without typing, so each holds the
/// session in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandoffCheck {
    pub(crate) auto_restart: bool,
    pub(crate) idle: Duration,
    pub(crate) idle_needed: Duration,
    pub(crate) in_voice: bool,
    pub(crate) watching_video: bool,
    pub(crate) composer_draft: bool,
}

impl HandoffCheck {
    pub(crate) fn ready(&self) -> bool {
        self.auto_restart
            && self.idle >= self.idle_needed
            && !self.in_voice
            && !self.watching_video
            && !self.composer_draft
    }
}
