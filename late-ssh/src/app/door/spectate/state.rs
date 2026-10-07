// Per-session spectating state: which live game this session watches, and the
// read-only stream doing it. A watch starts as a preview: the Games hub's
// rail sitting on one of its live rows, the watched screen alone where the
// selected card's landing would be. Enter opens it: the watched screen
// across the whole page with the watch chat docked beside it. A preview ends
// when the session leaves the hub. An open watch is a stop on the backtick
// cycle: leaving the hub steps away from it (`App::away_watches`, every one
// still streaming, so the hub shows its cards on the next visit), and it
// ends on Esc, when the game does, or once it has been off screen for
// `AWAY_WINDOW` (`App::tick`).
//
// `ChatLink` is a session's tie to a player's watch-chat room. A watch holds
// one (the pane beside the watched screen); a player holds one for their own
// running game (the line under it). `chat.rs` drives both.

use std::sync::Arc;
use std::time::{Duration, Instant};

use late_core::models::arcade_handle::{HANDLE_MAX_LEN, handle_shape_valid};
use late_core::models::leaderboard::DoorGame;
use uuid::Uuid;

use super::proxy::{LiveGame, SpectateProcess, WatchStatus, WatchTarget};
use crate::app::common::primitives::Screen;
use crate::render_signal::RenderSignal;

/// The doors whose hosts serve watch sessions. A new variant breaks the build
/// at its roster task, its watch target, its running game, its label, and
/// its live-strip picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpectateGame {
    Dcss,
    Nethack,
    Brogue,
}

impl SpectateGame {
    /// In the Games hub's card order, which is the order its rail lists
    /// their live games in.
    pub const ALL: [Self; 3] = [Self::Dcss, Self::Nethack, Self::Brogue];

    pub const fn door_game(self) -> DoorGame {
        match self {
            Self::Dcss => DoorGame::Dcss,
            Self::Nethack => DoorGame::Nethack,
            Self::Brogue => DoorGame::Brogue,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dcss => "DCSS",
            Self::Nethack => "NetHack",
            Self::Brogue => "Brogue",
        }
    }

    /// The door's own screen, where its player runs the game.
    pub const fn screen(self) -> Screen {
        match self {
            Self::Dcss => Screen::Dcss,
            Self::Nethack => Screen::Nethack,
            Self::Brogue => Screen::Brogue,
        }
    }

    /// The watchable door whose screen `screen` is, if any.
    pub fn of_screen(screen: Screen) -> Option<Self> {
        Self::ALL.into_iter().find(|game| game.screen() == screen)
    }

    /// `(cols, rows)`: the smallest screen the game draws its whole UI
    /// into. Watcher chat only takes room from a game, a player's own or a
    /// watched one, where the game keeps at least this much.
    pub const fn screen_min(self) -> (u16, u16) {
        match self {
            Self::Dcss | Self::Nethack => (80, 24),
            // Brogue's fixed grid.
            Self::Brogue => (100, 34),
        }
    }
}

/// How long an open watch stays up off screen. A watch is passive, so time
/// away from it, not time without a key, is what ends it.
pub const AWAY_WINDOW: Duration = Duration::from_secs(10 * 60);

/// One live game by its door and its player's handle, held inline so it is
/// `Copy`: the live strip's key for it (`LiveSource::DoorGame`). Built only
/// from a name in the arcade handle shape, which every playname a door host
/// lists is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiveGameKey {
    game: SpectateGame,
    handle: [u8; HANDLE_MAX_LEN],
    len: u8,
}

impl LiveGameKey {
    /// `None` for a name outside the handle shape.
    pub fn new(game: SpectateGame, playname: &str) -> Option<Self> {
        if !handle_shape_valid(playname) {
            return None;
        }
        let mut handle = [0; HANDLE_MAX_LEN];
        handle[..playname.len()].copy_from_slice(playname.as_bytes());
        Some(Self {
            game,
            handle,
            len: playname.len() as u8,
        })
    }

    pub fn game(&self) -> SpectateGame {
        self.game
    }

    pub fn playname(&self) -> &str {
        std::str::from_utf8(&self.handle[..usize::from(self.len)])
            .expect("a handle is ascii by construction")
    }
}

/// One live game on the Games hub's rail: which door it is in, and the host's
/// roster entry for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveRow {
    pub game: SpectateGame,
    pub entry: LiveGame,
}

/// One session's link to a player's watch-chat room: the room is resolved
/// once and joined once, then the link just carries its id. Pure bookkeeping;
/// `chat::tick` performs each step it hands out.
#[derive(Debug, PartialEq, Eq)]
pub struct ChatLink {
    game: SpectateGame,
    playname: String,
    resolve_requested: bool,
    room_id: Option<Uuid>,
}

/// What a link still owes.
#[derive(Debug, PartialEq, Eq)]
pub enum ChatLinkStep {
    /// Ask the service to resolve the room. Handed out once per link.
    Resolve,
    /// The room just resolved: join it. Handed out once per link.
    Join(Uuid),
    /// Nothing owed: still resolving, or already joined.
    Idle,
}

impl ChatLink {
    pub fn new(game: SpectateGame, playname: String) -> Self {
        Self {
            game,
            playname,
            resolve_requested: false,
            room_id: None,
        }
    }

    pub fn game(&self) -> SpectateGame {
        self.game
    }

    pub fn playname(&self) -> &str {
        &self.playname
    }

    /// The room, once it resolved and its join was requested.
    pub fn room_id(&self) -> Option<Uuid> {
        self.room_id
    }

    /// Advance the link. `resolved` is the service's current answer for this
    /// link's room.
    pub fn step(&mut self, resolved: Option<Uuid>) -> ChatLinkStep {
        match (self.room_id, resolved) {
            (Some(_), _) => ChatLinkStep::Idle,
            (None, Some(room_id)) => {
                self.room_id = Some(room_id);
                ChatLinkStep::Join(room_id)
            }
            (None, None) if self.resolve_requested => ChatLinkStep::Idle,
            (None, None) => {
                self.resolve_requested = true;
                ChatLinkStep::Resolve
            }
        }
    }
}

/// How much of a watch is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchMode {
    /// The rail sits on the live row: the watched screen alone, beside the
    /// rail. No chat.
    Preview,
    /// Enter on the live row: the watched screen across the whole page, the
    /// watch chat docked beside it.
    Open,
}

pub struct State {
    game: SpectateGame,
    playname: String,
    process: SpectateProcess,
    mode: WatchMode,
    /// When the watch last stepped off screen, for `AWAY_WINDOW`. Only read
    /// while it is off screen.
    seen_at: Instant,
    /// The watched player's chat room, beside their screen once the watch is
    /// open.
    chat: ChatLink,
}

impl State {
    pub fn new(
        game: SpectateGame,
        playname: String,
        target: WatchTarget,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        let process = SpectateProcess::spawn(game.door_game(), target, playname.clone(), repaint);
        Self {
            game,
            chat: ChatLink::new(game, playname.clone()),
            playname,
            process,
            mode: WatchMode::Preview,
            seen_at: Instant::now(),
        }
    }

    pub fn mode(&self) -> WatchMode {
        self.mode
    }

    pub fn is_open(&self) -> bool {
        self.mode == WatchMode::Open
    }

    /// Enter on the previewed row.
    pub fn open(&mut self) {
        self.mode = WatchMode::Open;
    }

    /// Esc out of an open watch: back to the preview beside the rail.
    pub fn close(&mut self) {
        self.mode = WatchMode::Preview;
    }

    /// This watch's row among the hub rail's live rows; `None` once the
    /// roster no longer lists the game.
    pub fn row_in(&self, live: &[LiveRow]) -> Option<usize> {
        live.iter()
            .position(|row| row.game == self.game && row.entry.playname == self.playname)
    }

    pub fn chat(&self) -> &ChatLink {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatLink {
        &mut self.chat
    }

    pub fn game(&self) -> SpectateGame {
        self.game
    }

    pub fn playname(&self) -> &str {
        &self.playname
    }

    /// The watched game as a key. Every playname a door host lists is a
    /// handle, and a watch is only started on a listed game.
    pub fn key(&self) -> LiveGameKey {
        LiveGameKey::new(self.game, &self.playname).expect("a watched playname is a handle")
    }

    /// Off screen: frames keep landing but no longer repaint the session,
    /// and `AWAY_WINDOW` counts from `now`.
    pub fn step_away(&mut self, now: Instant) {
        self.seen_at = now;
        self.process.set_on_screen(false);
    }

    /// Back on screen.
    pub fn resume(&mut self) {
        self.process.set_on_screen(true);
    }

    pub fn status(&self) -> WatchStatus {
        self.process.status()
    }

    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        self.process.with_screen(f)
    }

    /// Why this watch should end at `now`, if it should: a preview lives
    /// only on the Games hub, an open watch until it has been off screen
    /// (`on_hub` false: stepped away from) for `AWAY_WINDOW`, and any watch
    /// only while its stream is open.
    pub fn end_reason(&self, on_hub: bool, now: Instant) -> Option<WatchEnd> {
        let away = match on_hub {
            true => Duration::ZERO,
            false => now.saturating_duration_since(self.seen_at),
        };
        end_reason(self.mode, on_hub, away, self.status(), &self.playname)
    }
}

/// Why a watch ended.
#[derive(Debug, PartialEq, Eq)]
pub enum WatchEnd {
    /// The session left the Games hub with the watch still a preview.
    LeftHub,
    /// An open watch went `AWAY_WINDOW` without being on screen.
    WentAway,
    /// The stream closed: the game ended or the host went away.
    GameEnded(String),
}

fn end_reason(
    mode: WatchMode,
    on_hub: bool,
    away: Duration,
    status: WatchStatus,
    playname: &str,
) -> Option<WatchEnd> {
    match (mode, on_hub, status) {
        (WatchMode::Preview, false, _) => Some(WatchEnd::LeftHub),
        (_, _, WatchStatus::Ended) => Some(WatchEnd::GameEnded(playname.to_string())),
        (WatchMode::Open, false, WatchStatus::Connecting | WatchStatus::Watching)
            if away >= AWAY_WINDOW =>
        {
            Some(WatchEnd::WentAway)
        }
        (WatchMode::Preview, true, WatchStatus::Connecting | WatchStatus::Watching)
        | (WatchMode::Open, _, WatchStatus::Connecting | WatchStatus::Watching) => None,
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
