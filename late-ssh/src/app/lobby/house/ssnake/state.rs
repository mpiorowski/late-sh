use std::time::{Duration, Instant};

use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{SsnakeChipEvent, SsnakeChipEventKind, SsnakeService, SsnakeSnapshot};

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

/// One settled chip movement still on screen.
#[derive(Clone, Copy, Debug)]
pub struct ChipPop {
    pub delta: i64,
    pub kind: SsnakeChipEventKind,
    shown_at: Instant,
}

impl ChipPop {
    pub fn is_expired(&self) -> bool {
        self.shown_at.elapsed() >= CHIP_POP_TTL
    }
}

pub struct State {
    user_id: Uuid,
    snapshot: SsnakeSnapshot,
    svc: SsnakeService,
    snapshot_rx: watch::Receiver<SsnakeSnapshot>,
    chip_rx: broadcast::Receiver<SsnakeChipEvent>,
    /// This session's chip balance. While seated the arena is the authority
    /// (every chip move at this table is written here first and handed back),
    /// so the counter tracks each bite instead of waiting on the periodic
    /// leaderboard refresh.
    balance: i64,
    pops: Vec<ChipPop>,
}

impl State {
    pub fn new(svc: SsnakeService, user_id: Uuid, balance: i64) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let chip_rx = svc.subscribe_chip_events();
        Self {
            user_id,
            snapshot,
            svc,
            snapshot_rx,
            chip_rx,
            balance,
            pops: Vec::new(),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    /// Returns true when anything visible moved. The arena loop publishes at
    /// its speed cadence and self-terminates once the last snake leaves, so
    /// the snapshot peek covers all animation; the chip feed and pop expiry
    /// are polled here too, since a pop can outlive the last snapshot.
    pub fn tick(&mut self) -> bool {
        let mut changed = self.drain_chip_events();
        let before = self.pops.len();
        self.pops.retain(|pop| !pop.is_expired());
        changed |= self.pops.len() != before;
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            changed = true;
        }
        changed
    }

    /// Take every settled chip movement addressed to this session. Lagging
    /// is fine: pops are cosmetic and the balance on the newest event is
    /// still the true one, so a skipped batch costs a pop, not accuracy.
    fn drain_chip_events(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.chip_rx.try_recv() {
                Ok(event) => {
                    if event.user_id != self.user_id {
                        continue;
                    }
                    self.balance = event.balance;
                    self.pops.push(ChipPop {
                        delta: event.delta,
                        kind: event.kind,
                        shown_at: Instant::now(),
                    });
                    if self.pops.len() > MAX_CHIP_POPS {
                        self.pops.remove(0);
                    }
                    changed = true;
                }
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::debug!(skipped, "ssnake chip pop feed lagged");
                }
                Err(_) => break,
            }
        }
        changed
    }

    pub fn balance(&self) -> i64 {
        self.balance
    }

    pub fn set_balance(&mut self, balance: i64) {
        self.balance = balance;
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
