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
}

impl DoorGameId {
    pub fn key(self) -> &'static str {
        match self {
            Self::Lateania => "lateania",
            Self::GreenDragon => "greendragon",
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

    fn leave_active(&self, app: &mut App) -> bool;

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
