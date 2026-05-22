use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{TronService, TronSnapshot};

pub const SEAT_COUNT: usize = 4;
pub const BOARD_WIDTH: usize = 56;
pub const BOARD_HEIGHT: usize = 28;
pub const BOARD_CELLS: usize = BOARD_WIDTH * BOARD_HEIGHT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TronColor {
    Blue,
    Pink,
    Gold,
    Green,
}

impl TronColor {
    pub fn for_seat(index: usize) -> Self {
        match index {
            0 => Self::Blue,
            1 => Self::Pink,
            2 => Self::Gold,
            _ => Self::Green,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Pink => "Pink",
            Self::Gold => "Gold",
            Self::Green => "Green",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub fn delta(self) -> (i16, i16) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TronPhase {
    Waiting,
    Running,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TronOutcome {
    Winner { seat_index: usize },
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: u8,
    pub y: u8,
}

impl Position {
    pub fn index(self) -> usize {
        self.y as usize * BOARD_WIDTH + self.x as usize
    }
}

pub struct State {
    user_id: Uuid,
    snapshot: TronSnapshot,
    svc: TronService,
    snapshot_rx: watch::Receiver<TronSnapshot>,
}

impl State {
    pub fn new(svc: TronService, user_id: Uuid) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        Self {
            user_id,
            snapshot,
            svc,
            snapshot_rx,
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
    }

    pub fn snapshot(&self) -> &TronSnapshot {
        &self.snapshot
    }

    pub fn is_self(&self, user_id: Uuid) -> bool {
        self.user_id == user_id
    }

    pub fn seat_index(&self) -> Option<usize> {
        self.snapshot
            .seats
            .iter()
            .position(|seat| *seat == Some(self.user_id))
    }

    pub fn user_color(&self) -> Option<TronColor> {
        self.seat_index().map(TronColor::for_seat)
    }

    pub fn sit(&self) {
        self.svc.sit_task(self.user_id);
    }

    pub fn leave_seat(&self) {
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn start_round(&self) {
        self.svc.start_round_task(self.user_id);
    }

    pub fn steer(&self, direction: Direction) {
        self.svc.steer_task(self.user_id, direction);
    }

    pub fn touch_activity(&self) {
        if self.seat_index().is_some() {
            self.svc.touch_activity_task(self.user_id);
        }
    }
}
