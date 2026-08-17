use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{SsnakeChipKind, SsnakeService, SsnakeSnapshot};

/// Seat arrays are always this size; the per-room seat count (2-5) lives in
/// the table settings and unused trailing seats simply stay empty.
pub const MAX_SEATS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakeColor {
    Green,
    Red,
    Blue,
    Purple,
    Cyan,
}

impl SsnakeColor {
    pub fn for_seat(index: usize) -> Self {
        match index {
            0 => Self::Green,
            1 => Self::Red,
            2 => Self::Blue,
            3 => Self::Purple,
            _ => Self::Cyan,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Red => "Red",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Cyan => "Cyan",
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
/// the original: the snake sits still until its first steer. Only `Moving`
/// snakes count toward the payout multiplier, so parking a body on the board
/// earns nobody anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Idle,
    Moving(Direction),
    Dying,
}

/// The arena never ends; it only sleeps. `Idle` is an empty board nobody is
/// sitting at, `Running` is the tick loop turning. There is no match to win,
/// start, or finish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakePhase {
    Idle,
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

/// How long a `+5 chips` pop stays on screen. Long enough to read between
/// bites, short enough that a fast eater's pops do not pile up.
pub const CHIP_POP_TTL: Duration = Duration::from_millis(2500);
/// Only the newest few pops are worth showing; anything older has been read.
const MAX_CHIP_POPS: usize = 3;

/// One chip movement still on screen.
#[derive(Clone, Copy, Debug)]
pub struct ChipPop {
    pub delta: i64,
    pub kind: SsnakeChipKind,
    shown_at: Instant,
}

impl ChipPop {
    pub fn is_expired(&self) -> bool {
        self.shown_at.elapsed() >= CHIP_POP_TTL
    }
}

pub struct State {
    user_id: Uuid,
    snapshot: Arc<SsnakeSnapshot>,
    svc: SsnakeService,
    snapshot_rx: watch::Receiver<Arc<SsnakeSnapshot>>,
    /// Our seat's running tally as of the last snapshot we popped from. The
    /// difference against the next one is the movement to pop.
    last_net: i64,
    pops: Vec<ChipPop>,
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
            last_net: 0,
            pops: Vec::new(),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    /// Returns true when anything visible moved. The arena loop publishes at
    /// its speed cadence and self-terminates once the last snake leaves, so
    /// the snapshot peek covers all animation; pop expiry is polled here too,
    /// since a pop can outlive the last snapshot.
    pub fn tick(&mut self) -> bool {
        let before = self.pops.len();
        self.pops.retain(|pop| !pop.is_expired());
        let mut changed = self.pops.len() != before;
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.pop_own_movement();
            changed = true;
        }
        changed
    }

    /// Pop what the arena just paid (or charged) this session. Our seat's
    /// running tally is already in the shared snapshot, so the delta between
    /// two snapshots *is* the movement: no per-user event channel needed, and
    /// a seat we no longer hold simply resets the baseline.
    fn pop_own_movement(&mut self) {
        let Some(seat) = self.seat_index() else {
            self.last_net = 0;
            return;
        };
        let player = &self.snapshot.players[seat];
        let net = player.chips;
        if net == self.last_net {
            return;
        }
        let delta = net - self.last_net;
        self.last_net = net;
        let Some(kind) = player.last_chip else {
            return;
        };
        self.pops.push(ChipPop {
            delta,
            kind,
            shown_at: Instant::now(),
        });
        if self.pops.len() > MAX_CHIP_POPS {
            self.pops.remove(0);
        }
    }

    /// Live pops, oldest first.
    pub fn chip_pops(&self) -> &[ChipPop] {
        &self.pops
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

    pub fn steer(&self, direction: Direction) {
        self.svc.steer_task(self.user_id, direction);
    }

    pub fn vote_skip(&self) {
        self.svc.vote_skip_task(self.user_id);
    }

    /// Whether this session already has a live vote against the current
    /// arena, so the sidebar can say "voted" rather than re-offer the key.
    pub fn has_voted_skip(&self) -> bool {
        self.seat_index()
            .is_some_and(|seat| self.snapshot.skip_votes[seat])
    }

    pub fn touch_activity(&self) {
        if self.seat_index().is_some() {
            self.svc.touch_activity_task(self.user_id);
        }
    }
}
