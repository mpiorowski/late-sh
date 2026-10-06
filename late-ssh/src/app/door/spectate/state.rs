// Per-session spectating state: which live game this session watches, and the
// read-only stream doing it. Watching lives inside the Games hub: while a
// `State` is held the hub draws the watched screen instead of its sidebar and
// landing, and leaving the hub ends the watch (`App::tick`).
//
// `ChatLink` is a session's tie to a player's watch-chat room. A watch holds
// one (the pane beside the watched screen); a player holds one for their own
// running game (the line under it). `chat.rs` drives both.

use std::sync::Arc;

use late_core::models::leaderboard::DoorGame;
use uuid::Uuid;

use super::proxy::{LiveGame, SpectateProcess, WatchStatus, WatchTarget};
use crate::render_signal::RenderSignal;

/// The doors whose hosts serve watch sessions. A new variant breaks the build
/// at its roster task, its watch target, and its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpectateGame {
    Dcss,
}

impl SpectateGame {
    pub const fn door_game(self) -> DoorGame {
        match self {
            Self::Dcss => DoorGame::Dcss,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dcss => "DCSS",
        }
    }
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

pub struct State {
    game: SpectateGame,
    playname: String,
    process: SpectateProcess,
    /// The watched player's chat room, beside their screen.
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
        }
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

    pub fn status(&self) -> WatchStatus {
        self.process.status()
    }

    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        self.process.with_screen(f)
    }

    /// Why this watch should end now, if it should: a watch lives only on the
    /// Games hub, and only while its stream is open.
    pub fn end_reason(&self, on_hub: bool) -> Option<WatchEnd> {
        end_reason(on_hub, self.status(), &self.playname)
    }
}

/// Why a watch ended.
#[derive(Debug, PartialEq, Eq)]
pub enum WatchEnd {
    /// The session left the Games hub.
    LeftHub,
    /// The stream closed: the game ended or the host went away.
    GameEnded(String),
}

fn end_reason(on_hub: bool, status: WatchStatus, playname: &str) -> Option<WatchEnd> {
    match (on_hub, status) {
        (false, _) => Some(WatchEnd::LeftHub),
        (true, WatchStatus::Ended) => Some(WatchEnd::GameEnded(playname.to_string())),
        (true, WatchStatus::Connecting | WatchStatus::Watching) => None,
    }
}

/// The game to switch to from `current`, one step forward or back through
/// the roster, wrapping at the ends. `None` when there is nobody else to
/// watch. A `current` that has left the roster steps from the start.
pub fn step_target<'a>(roster: &'a [LiveGame], current: &str, forward: bool) -> Option<&'a str> {
    let others = roster.iter().filter(|g| g.playname != current).count();
    if others == 0 {
        return None;
    }
    let len = roster.len();
    let next = match roster.iter().position(|g| g.playname == current) {
        Some(at) if forward => (at + 1) % len,
        Some(at) => (at + len - 1) % len,
        None if forward => 0,
        None => len - 1,
    };
    Some(roster[next].playname.as_str())
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
