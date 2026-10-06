// Per-session spectating state: which live game this session watches, and the
// read-only stream doing it. Watching lives inside the Games hub: while a
// `State` is held the hub draws the watched screen instead of its sidebar and
// landing, and leaving the hub ends the watch (`App::tick`).

use std::sync::Arc;

use late_core::models::leaderboard::DoorGame;

use super::proxy::{LiveGame, SpectateProcess, WatchStatus, WatchTarget};
use crate::render_signal::RenderSignal;

/// The doors whose hosts serve watch sessions. A new variant breaks the build
/// at its roster task, its watch target, and its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

pub struct State {
    game: SpectateGame,
    playname: String,
    process: SpectateProcess,
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
            playname,
            process,
        }
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
