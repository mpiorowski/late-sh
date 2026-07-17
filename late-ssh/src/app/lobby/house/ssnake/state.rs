use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{SsnakeService, SsnakeSnapshot};

/// Seat arrays are always this size; the per-room seat count (2-4) lives in
/// the table settings and unused trailing seats simply stay empty.
pub const MAX_SEATS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakeColor {
    Green,
    Red,
    Blue,
    Purple,
}

impl SsnakeColor {
    pub fn for_seat(index: usize) -> Self {
        match index {
            0 => Self::Green,
            1 => Self::Red,
            2 => Self::Blue,
            _ => Self::Purple,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Red => "Red",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
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

/// What a snake is doing this tick. `Idle` is the just-(re)spawned state from
/// the original: the snake sits still until its first steer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Idle,
    Moving(Direction),
    Dying,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakePhase {
    Waiting,
    Running,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakeOutcome {
    Winner { seat_index: usize },
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

pub struct State {
    user_id: Uuid,
    snapshot: SsnakeSnapshot,
    svc: SsnakeService,
    snapshot_rx: watch::Receiver<SsnakeSnapshot>,
}

impl State {
    pub fn new(svc: SsnakeService, user_id: Uuid) -> Self {
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

    pub fn snapshot(&self) -> &SsnakeSnapshot {
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

    pub fn user_color(&self) -> Option<SsnakeColor> {
        self.seat_index().map(SsnakeColor::for_seat)
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

    pub fn select_arena(&self, delta: isize) {
        self.svc.select_level_task(self.user_id, delta);
    }

    pub fn touch_activity(&self) {
        if self.seat_index().is_some() {
            self.svc.touch_activity_task(self.user_id);
        }
    }
}
