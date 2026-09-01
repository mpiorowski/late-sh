use ratatui::{Frame, layout::Rect};
use uuid::Uuid;

use crate::app::{
    activity::event::{ActivityEvent, ActivityGame},
    files::terminal_image::TerminalImageFrame,
    state::App,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DoorGameId {
    Lateania,
    GreenDragon,
    Darkroom,
}

impl DoorGameId {
    pub fn key(self) -> &'static str {
        match self {
            Self::Lateania => "lateania",
            Self::GreenDragon => "greendragon",
            Self::Darkroom => "darkroom",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoorGameOutcome {
    Won,
    Lost,
    Completed,
    Abandoned,
}

pub enum DoorGameEvent {
    Activity(ActivityEvent),
    Outcome {
        user_id: Uuid,
        game_id: DoorGameId,
        outcome: DoorGameOutcome,
        detail: Option<String>,
        score: Option<i32>,
    },
}

/// How long a native door keeps its loaded state while the player is away:
/// no key handled by the door, and the door not the open screen (`App::tick`
/// stamps presence while it is). Dark Room and Green Dragon survive a screen
/// switch (they tick off-screen), so without a deadline an abandoned door
/// would advertise itself on the backtick cycle for the rest of the session.
/// Past this the visit is over: the door saves and drops exactly as an
/// explicit leave does.
pub const IDLE_WINDOW: std::time::Duration = std::time::Duration::from_secs(30 * 60);

pub trait DoorGame {
    type View<'a>;

    fn id(&self) -> DoorGameId;

    fn title(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn activity_game(&self) -> Option<ActivityGame> {
        None
    }

    fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        view: &Self::View<'_>,
        terminal_images: &mut TerminalImageFrame,
    );

    fn handle_key(&self, app: &mut App, byte: u8) -> bool;

    fn handle_arrow(&self, app: &mut App, key: u8) -> bool;

    /// Handle a mouse event (click/scroll). Doors that don't use the mouse keep
    /// the default no-op; Lateania overrides it to run its clickable action bar.
    fn handle_mouse(&self, app: &mut App, mouse: crate::app::input::MouseEvent) -> bool {
        let _ = (app, mouse);
        false
    }

    fn activity_for_outcome(
        &self,
        user_id: Uuid,
        username: impl Into<String>,
        outcome: DoorGameOutcome,
        detail: Option<String>,
        score: Option<i32>,
    ) -> Option<ActivityEvent> {
        match (self.activity_game(), outcome) {
            (Some(game), DoorGameOutcome::Won) => Some(ActivityEvent::game_won(
                user_id, username, game, detail, score,
            )),
            _ => None,
        }
    }
}
