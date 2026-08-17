//! The perpetual Super Snake arena.
//!
//! There is no match here: one board runs forever, anyone may sit down or
//! stand up mid-flight, and the arena tallies what each seat is owed as it
//! plays. Clearing an arena of food reshuffles it to another random level and
//! the same snakes keep going.
//!
//! Every payout scales with the number of snakes *moving* on the board, so a
//! lone farmer earns the base rate and a busy arena pays everyone several
//! times over. Seated-but-parked snakes deliberately count for nothing —
//! otherwise a table of idlers would be worth more than a table of players.
//!
//! The tally is in-memory and reaches the ledger exactly once, when the seat
//! is given up (by hand or by the idle kick). A bite is worth single-digit
//! chips and lands every few seconds, so writing each one would turn a busy
//! table into the app's loudest source of `user_chips` writes, and every one
//! of those fans out through the `chip_user_changed` notify into a full shop
//! snapshot reload for that player. One row per visit instead.

use std::{
    cmp::Ordering,
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use late_core::models::chips::ChipMove;
use rand::Rng;
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    games::chips::svc::ChipService,
    lobby::house::{
        ssnake::{
            levels::{LEVELS, SsnakeLevel},
            settings::{
                SSNAKE_ARENA_FREEZE, SSNAKE_BONUS_FOOD_ODDS, SSNAKE_CLEAR_CHIPS,
                SSNAKE_CRASH_CHIPS, SSNAKE_DEATH_SHRINK_PER_TICK, SSNAKE_SKIP_COOLDOWN,
                SSNAKE_SKIP_VOTE_TTL, SsnakeTableSettings, ssnake_food_chips,
                ssnake_respawn_length,
            },
            state::{Direction, MAX_SEATS, Motion, Pos, SsnakeColor, SsnakePhase},
        },
        types::RoomGameEvent,
    },
};

/// Deliberately shorter than the other house tables' five minutes: the arena
/// is small and always live, so a seat nobody is touching has to go back into
/// circulation fast. A player who is genuinely steering resets this several
/// times a second.
const SEAT_IDLE_TIMEOUT_SECS: u64 = 2 * 60;
/// Original `MaxSnakePoints`; caps body length plus pending growth.
const MAX_SNAKE_LEN: i32 = 500;
/// How many free cells to sample when placing a joining or respawning snake.
/// The winner is the candidate whose nearest rival head is farthest away, so
/// a newcomer lands in an empty pocket instead of on top of the pack.
const SPAWN_CANDIDATES: usize = 96;
/// Cadence used when no level is loaded at all (every asset failed to parse).
const FALLBACK_TICK_MILLIS: u64 = 150;

#[derive(Clone)]
pub struct SsnakeService {
    room_id: Uuid,
    chip_svc: ChipService,
    settings: SsnakeTableSettings,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    /// Published behind an `Arc` because every session that has opened this
    /// table keeps its client for the rest of the session and drains this
    /// channel on every app tick, on screen or not. The arena publishes at
    /// its own cadence for as long as one seat is held, so a bare snapshot
    /// would have every such session deep-copying five snake bodies several
    /// times a second to look at nothing.
    snapshot_tx: watch::Sender<Arc<SsnakeSnapshot>>,
    snapshot_rx: watch::Receiver<Arc<SsnakeSnapshot>>,
    state: Arc<Mutex<SharedState>>,
}

/// What last moved a seat's tally, so the pop can say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SsnakeChipKind {
    Food,
    BonusFood,
    ArenaClear,
    Crash,
}

#[derive(Clone, Debug)]
pub struct SsnakePlayerSnapshot {
    pub body: Vec<Pos>,
    pub motion: Motion,
    /// Net chips this seat has run up since sitting down; goes negative when
    /// crashes outrun food. Unbanked: it reaches the ledger when the seat is
    /// given up, and the owning session pops the difference between two
    /// snapshots as it moves.
    pub chips: i64,
    /// What moved `chips` last, for the pop label.
    pub last_chip: Option<SsnakeChipKind>,
    pub seated: bool,
}

#[derive(Clone, Debug)]
pub struct SsnakeSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; MAX_SEATS],
    /// How many of the seats this table opens (2-5, from settings).
    pub seat_limit: usize,
    pub level: Option<Arc<SsnakeLevel>>,
    pub players: [SsnakePlayerSnapshot; MAX_SEATS],
    pub point: Option<Pos>,
    /// The food on the board is the pink one, worth a multiple of the usual.
    pub bonus_food: bool,
    /// Arena walls orthogonally touching the food, 0-4. Drives the edge bonus.
    pub food_wall_edges: i64,
    pub points_left: i32,
    pub phase: SsnakePhase,
    /// Snakes currently slithering — the live payout multiplier.
    pub moving_snakes: i64,
    /// Time left on the post-reshuffle hold; 0 once play resumes.
    pub freeze_millis_left: u64,
    /// Per seat, whether that player has a live vote to skip this arena.
    pub skip_votes: [bool; MAX_SEATS],
    /// Time left before the arena may be skipped again; 0 when it is ready.
    pub skip_cooldown_millis_left: u64,
    pub status_message: String,
    pub speed_label: String,
    pub tick_count: u32,
}

impl SsnakeSnapshot {
    /// What the food on the board is worth right now — edge bonus, pink
    /// multiple and moving-snake count all folded in.
    pub fn food_chips(&self) -> i64 {
        ssnake_food_chips(self.food_wall_edges, self.bonus_food, self.moving_snakes)
    }

    /// What clearing the arena is worth right now.
    pub fn clear_chips(&self) -> i64 {
        SSNAKE_CLEAR_CHIPS * self.moving_snakes.max(1)
    }

    /// True while the board is held after a reshuffle.
    pub fn is_frozen(&self) -> bool {
        self.freeze_millis_left > 0
    }

    /// Whole seconds left on the hold, rounded up, for the countdown.
    pub fn freeze_secs_left(&self) -> u64 {
        self.freeze_millis_left.div_ceil(1_000)
    }

    /// The food on the board is the one that clears the arena.
    pub fn is_final_food(&self) -> bool {
        self.point.is_some() && self.points_left <= 1
    }

    /// Votes cast, and how many it takes — every seated player.
    pub fn skip_tally(&self) -> (usize, usize) {
        let cast = self.skip_votes.iter().filter(|voted| **voted).count();
        let seated = self.seats.iter().filter(|seat| seat.is_some()).count();
        (cast, seated)
    }

    pub fn skip_cooldown_secs_left(&self) -> u64 {
        self.skip_cooldown_millis_left.div_ceil(1_000)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickLoop {
    generation: u64,
}

/// A seat's final figure on its way to the ledger. The sign lives in the
/// `ChipMove`, so a losing visit and a winning one travel the same path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Settlement {
    user_id: Uuid,
    chips: i64,
    chip_move: ChipMove,
}

impl Settlement {
    /// `None` when the seat comes out exactly even: nothing moved, so nothing
    /// is written and nobody is notified.
    fn for_net(user_id: Uuid, net: i64) -> Option<Self> {
        match net.cmp(&0) {
            Ordering::Greater => Some(Self {
                user_id,
                chips: net,
                chip_move: ChipMove::SsnakeArenaEarned,
            }),
            Ordering::Less => Some(Self {
                user_id,
                chips: -net,
                chip_move: ChipMove::SsnakeArenaLost,
            }),
            Ordering::Equal => None,
        }
    }
}

/// What an idle-kick attempt did. A kick that found nothing to reclaim
/// changed no state, so it must not publish: the kick timer is re-armed on
/// every keypress, and a publish per keystroke would wake every session
/// watching the table.
#[derive(Default)]
struct KickOutcome {
    reclaimed: bool,
    settlement: Option<Settlement>,
}

#[derive(Clone)]
pub struct SsnakeServiceContext {
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
}

impl SsnakeService {
    pub fn new_with_events(
        room_id: Uuid,
        chip_svc: ChipService,
        settings: SsnakeTableSettings,
        context: SsnakeServiceContext,
    ) -> Self {
        let SsnakeServiceContext { room_event_tx } = context;
        let state = SharedState::new(room_id, settings);
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(state.snapshot()));
        Self {
            room_id,
            chip_svc,
            settings,
            room_event_tx,
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<Arc<SsnakeSnapshot>> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> Arc<SsnakeSnapshot> {
        self.snapshot_rx.borrow().clone()
    }

    pub fn settings(&self) -> SsnakeTableSettings {
        self.settings
    }

    /// Sitting down is the only "start": the snake drops into the running
    /// board immediately and the tick loop spins up if it was asleep.
    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, seat_joined, tick_loop) = {
                let mut state = svc.state.lock().await;
                let seat_joined = state.sit(user_id);
                let tick_loop = state.wake_arena();
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, seat_joined, tick_loop)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.schedule_tick_loop(tick_loop);
            if seat_joined.is_some() {
                let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                    room_id: svc.room_id,
                    user_id,
                });
            }
        });
    }

    pub fn leave_seat_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let settlement = {
                let mut state = svc.state.lock().await;
                let settlement = state.leave(user_id, true);
                svc.publish(&state);
                settlement
            };
            svc.settle(settlement);
        });
    }

    pub fn steer_task(&self, user_id: Uuid, direction: Direction) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.steer(user_id, direction);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn vote_skip_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.vote_skip(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn touch_activity_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.record_activity(user_id)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    /// The arena's heartbeat. It re-reads the cadence every iteration, so an
    /// arena shuffle to a level with different pacing takes effect without
    /// restarting the loop, and it exits the moment the board empties.
    fn schedule_tick_loop(&self, tick_loop: Option<TickLoop>) {
        let Some(tick_loop) = tick_loop else {
            return;
        };
        let svc = self.clone();
        tokio::spawn(async move {
            let mut millis = { svc.state.lock().await.tick_millis() };
            loop {
                tokio::time::sleep(Duration::from_millis(millis)).await;
                let outcome = {
                    let mut state = svc.state.lock().await;
                    let outcome = state.tick(tick_loop.generation);
                    if outcome.ticked {
                        svc.publish(&state);
                    }
                    outcome
                };
                match outcome.next_millis {
                    Some(next) => millis = next,
                    None => break,
                }
            }
        });
    }

    fn schedule_inactivity_kick(&self, user_id: Uuid, activity_generation: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)).await;
            let outcome = {
                let mut state = svc.state.lock().await;
                let outcome = state.kick_inactive_user(user_id, activity_generation);
                if outcome.reclaimed {
                    svc.publish(&state);
                }
                outcome
            };
            svc.settle(outcome.settlement);
        });
    }

    fn publish(&self, state: &SharedState) {
        let _ = self.snapshot_tx.send(Arc::new(state.snapshot()));
    }

    /// Bank a seat's final figure. This is the arena's only ledger write, and
    /// the idle kick reaches it as surely as the `l` key does: nothing frees
    /// a seat when a session drops, so for anyone who just closes their
    /// terminal the kick *is* the settle path.
    ///
    /// A declined debit (a player who has spent down to nothing since they
    /// sat) is expected rather than an error: the arena is not a creditor.
    fn settle(&self, settlement: Option<Settlement>) {
        let Some(settlement) = settlement else {
            return;
        };
        let chip_svc = self.chip_svc.clone();
        tokio::spawn(async move {
            match chip_svc
                .apply_move(settlement.user_id, settlement.chip_move, settlement.chips)
                .await
            {
                Ok(Some(_)) => {}
                Ok(None) => tracing::debug!(
                    user_id = %settlement.user_id,
                    chips = settlement.chips,
                    reason = settlement.chip_move.reason(),
                    "ssnake settlement declined by the balance floor"
                ),
                Err(error) => tracing::error!(
                    ?error,
                    user_id = %settlement.user_id,
                    chips = settlement.chips,
                    reason = settlement.chip_move.reason(),
                    "failed to settle ssnake seat"
                ),
            }
        });
    }
}

#[derive(Default)]
struct TickOutcome {
    ticked: bool,
    /// Cadence for the next sleep; `None` retires the loop.
    next_millis: Option<u64>,
}

#[derive(Clone, Debug)]
struct PlayerState {
    body: VecDeque<Pos>,
    /// Segments still owed to the body (original `S1Left`). Grows on eats,
    /// pays out one segment per move.
    pending_growth: i32,
    motion: Motion,
    /// Direction actually applied on the previous move (original `OldDir`);
    /// second half of the reversal guard.
    last_moved: Option<Direction>,
    /// Net chips run up since sitting down, still unbanked. Survives arena
    /// shuffles and crashes; standing up banks it and clears it.
    chips: i64,
    /// What moved `chips` last, for the owner's pop.
    last_chip: Option<SsnakeChipKind>,
    /// Length to regrow to after the death shrink (original `S1OldLength`).
    respawn_length: i32,
}

impl PlayerState {
    fn empty() -> Self {
        Self {
            body: VecDeque::new(),
            pending_growth: 0,
            motion: Motion::Idle,
            last_moved: None,
            chips: 0,
            last_chip: None,
            respawn_length: 0,
        }
    }

    fn snapshot(&self, seated: bool) -> SsnakePlayerSnapshot {
        SsnakePlayerSnapshot {
            body: self.body.iter().copied().collect(),
            motion: self.motion,
            chips: self.chips,
            last_chip: self.last_chip,
            seated,
        }
    }
}

fn empty_players() -> [PlayerState; MAX_SEATS] {
    std::array::from_fn(|_| PlayerState::empty())
}

struct SharedState {
    room_id: Uuid,
    settings: SsnakeTableSettings,
    seats: [Option<Uuid>; MAX_SEATS],
    /// Open seats on this table (2-5, from settings).
    seat_limit: usize,
    last_activity: [Instant; MAX_SEATS],
    activity_generation: [u64; MAX_SEATS],
    level: Option<Arc<SsnakeLevel>>,
    players: [PlayerState; MAX_SEATS],
    point: Option<Pos>,
    bonus_food: bool,
    /// Walls touching the current food; recomputed every time one spawns.
    food_wall_edges: i64,
    points_left: i32,
    phase: SsnakePhase,
    /// Set when the arena reshuffles: until it passes, nobody moves and
    /// steers are dropped rather than queued.
    frozen_until: Option<Instant>,
    /// When each seat cast its skip vote, for the vote TTL. Cleared whenever
    /// the arena changes or the seat set does — both invalidate a tally
    /// taken against a different table.
    skip_votes: [Option<Instant>; MAX_SEATS],
    /// Earliest the arena may be vote-skipped again.
    skip_available_at: Option<Instant>,
    status_message: String,
    /// Bumped whenever the tick loop must be replaced; a loop whose
    /// generation no longer matches retires itself.
    tick_generation: u64,
    tick_count: u32,
}

impl SharedState {
    fn new(room_id: Uuid, settings: SsnakeTableSettings) -> Self {
        let now = Instant::now();
        let mut state = Self {
            room_id,
            settings,
            seats: [None; MAX_SEATS],
            seat_limit: settings.seats.clamp(2, MAX_SEATS),
            last_activity: [now; MAX_SEATS],
            activity_generation: [0; MAX_SEATS],
            level: None,
            players: empty_players(),
            point: None,
            bonus_food: false,
            food_wall_edges: 0,
            points_left: 0,
            phase: SsnakePhase::Idle,
            frozen_until: None,
            skip_votes: [None; MAX_SEATS],
            skip_available_at: None,
            status_message: "Take a seat — the arena is always open.".to_string(),
            tick_generation: 0,
            tick_count: 0,
        };
        // The board exists before anyone sits at it, so passers-by see an
        // arena with food on it rather than a splash screen.
        state.load_level();
        state
    }

    fn snapshot(&self) -> SsnakeSnapshot {
        SsnakeSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            seat_limit: self.seat_limit,
            level: self.level.clone(),
            players: std::array::from_fn(|index| {
                self.players[index].snapshot(self.seats[index].is_some())
            }),
            point: self.point,
            bonus_food: self.bonus_food,
            food_wall_edges: self.food_wall_edges,
            points_left: self.points_left,
            phase: self.phase,
            moving_snakes: self.moving_snakes(),
            freeze_millis_left: self.freeze_millis_left(),
            skip_votes: std::array::from_fn(|index| self.has_live_vote(index)),
            skip_cooldown_millis_left: self.skip_cooldown_millis_left(),
            status_message: self.status_message.clone(),
            speed_label: self.settings.speed.label().to_string(),
            tick_count: self.tick_count,
        }
    }

    fn tick_millis(&self) -> u64 {
        self.level
            .as_ref()
            .map(|level| self.settings.speed.scale_tick(level.tick_millis))
            .unwrap_or(FALLBACK_TICK_MILLIS)
    }

    /// The payout multiplier: seated snakes that are actually slithering. A
    /// snake that has not been steered since it spawned (or is mid-death) is
    /// not playing, so sitting still can never inflate anyone's payout.
    fn moving_snakes(&self) -> i64 {
        self.players
            .iter()
            .enumerate()
            .filter(|(seat_index, player)| {
                self.seats[*seat_index].is_some() && matches!(player.motion, Motion::Moving(_))
            })
            .count() as i64
    }

    // ── Seats ──────────────────────────────────────────────────

    fn sit(&mut self, user_id: Uuid) -> Option<usize> {
        if self.seats.contains(&Some(user_id)) {
            return None;
        }
        let Some(index) = self
            .seats
            .iter()
            .take(self.seat_limit)
            .position(Option::is_none)
        else {
            self.status_message = "All snakes are taken.".to_string();
            return None;
        };
        self.seats[index] = Some(user_id);
        self.players[index] = PlayerState::empty();
        self.spawn_snake(index);
        // The bar for consensus just moved, so a tally taken against the old
        // table no longer means what it said.
        self.skip_votes = [None; MAX_SEATS];
        self.status_message = format!(
            "{} joined the arena — steer to start earning.",
            SsnakeColor::for_seat(index).label()
        );
        Some(index)
    }

    /// Free a seat and hand back what it owes or is owed. `voluntary` marks a
    /// player pressing `l` rather than the idle timer reclaiming the seat.
    ///
    /// Standing up mid-slither costs the crash penalty, because otherwise it
    /// *is* the crash-avoidance move: a snake one tick from a wall could bail
    /// for free and sit straight back down on a clean spawn. Parked (never
    /// steered) and dying snakes pay nothing — the first was never at risk
    /// and the second has already been charged for the crash it is playing
    /// out. The idle kick never charges: it is involuntary and two minutes
    /// away, so it cannot be used to dodge anything.
    fn leave(&mut self, user_id: Uuid, voluntary: bool) -> Option<Settlement> {
        let Some(index) = self.seat_index(user_id) else {
            return None;
        };
        let bailed = voluntary && matches!(self.players[index].motion, Motion::Moving(_));
        if bailed {
            self.charge(index, SSNAKE_CRASH_CHIPS, SsnakeChipKind::Crash);
        }
        // Read after the bail penalty: the whole visit, including the exit
        // charge, is what lands in the ledger.
        let settlement = Settlement::for_net(user_id, self.players[index].chips);
        self.seats[index] = None;
        self.players[index] = PlayerState::empty();
        self.skip_votes = [None; MAX_SEATS];
        if self.seated_count() == 0 {
            // Nobody left to feed: park the board and retire the loop. The
            // level and its food stay on screen for spectators.
            self.phase = SsnakePhase::Idle;
            self.tick_generation = self.tick_generation.wrapping_add(1);
            self.status_message = "Take a seat — the arena is always open.".to_string();
        } else {
            let color = SsnakeColor::for_seat(index).label();
            self.status_message = match bailed {
                true => format!("{color} bailed out (-{SSNAKE_CRASH_CHIPS})."),
                false => format!("{color} left the arena."),
            };
        }
        settlement
    }

    /// Start the heartbeat if the arena has a snake and is not already
    /// running. Returns the loop to spawn, or `None` when nothing changed.
    fn wake_arena(&mut self) -> Option<TickLoop> {
        if self.phase == SsnakePhase::Running || self.seated_count() == 0 {
            return None;
        }
        self.phase = SsnakePhase::Running;
        self.tick_generation = self.tick_generation.wrapping_add(1);
        Some(TickLoop {
            generation: self.tick_generation,
        })
    }

    fn steer(&mut self, user_id: Uuid, direction: Direction) {
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat to steer.".to_string();
            return;
        };
        // Dropped, not queued: the whole point of the hold is that a key
        // pressed on the old arena must not decide your first move on the
        // new one.
        if self.is_frozen() {
            return;
        }
        let player = &mut self.players[index];
        match player.motion {
            Motion::Idle => player.motion = Motion::Moving(direction),
            Motion::Moving(current) => {
                if direction != current.opposite()
                    && player.last_moved != Some(direction.opposite())
                {
                    player.motion = Motion::Moving(direction);
                }
            }
            Motion::Dying => {}
        }
    }

    // ── Simulation ─────────────────────────────────────────────

    fn tick(&mut self, generation: u64) -> TickOutcome {
        if self.phase != SsnakePhase::Running || self.tick_generation != generation {
            return TickOutcome::default();
        }
        // The counter keeps running through the hold so the food keeps
        // blinking and the countdown repaints.
        self.tick_count = self.tick_count.wrapping_add(1);
        if self.is_frozen() {
            return TickOutcome {
                ticked: true,
                next_millis: Some(self.tick_millis()),
            };
        }

        // The original moves and collision-checks player 1, then player 2, so
        // later snakes see earlier snakes' fresh positions. Keep that order.
        for seat_index in 0..MAX_SEATS {
            if self.step_player(seat_index) {
                // The arena just reshuffled under everyone; the remaining
                // snakes are freshly placed and hold still until steered.
                break;
            }
        }

        TickOutcome {
            ticked: true,
            next_millis: Some(self.tick_millis()),
        }
    }

    /// Returns true when this step cleared the arena and reshuffled it.
    fn step_player(&mut self, seat_index: usize) -> bool {
        if self.seats[seat_index].is_none() {
            return false;
        }
        // Self-heal a snake that could not be placed earlier (packed board).
        if self.players[seat_index].body.is_empty() {
            self.spawn_snake(seat_index);
            return false;
        }
        match self.players[seat_index].motion {
            Motion::Idle => false,
            Motion::Dying => {
                self.step_death_shrink(seat_index);
                false
            }
            Motion::Moving(direction) => self.step_move(seat_index, direction),
        }
    }

    /// The original death animation, sped up: drop several tail segments per
    /// tick instead of one, then respawn at the previous size. With lives
    /// gone the crash costs chips, not a life, so a snake always comes back —
    /// and a long snake's wake should not lock the player out for the length
    /// of its own body.
    fn step_death_shrink(&mut self, seat_index: usize) {
        for _ in 0..SSNAKE_DEATH_SHRINK_PER_TICK {
            if self.players[seat_index].body.len() <= 1 {
                break;
            }
            self.players[seat_index].body.pop_back();
        }
        if self.players[seat_index].body.len() > 1 {
            return;
        }
        let respawn_length = self.players[seat_index].respawn_length;
        self.spawn_snake(seat_index);
        self.players[seat_index].pending_growth = respawn_length.min(MAX_SNAKE_LEN - 1);
    }

    fn step_move(&mut self, seat_index: usize, direction: Direction) -> bool {
        let Some(level) = self.level.clone() else {
            return false;
        };
        let Some(&head) = self.players[seat_index].body.front() else {
            return false;
        };
        let new_head = wrap_pos(head, direction, level.width, level.height);

        // Move first, verify after: matches the original MoveSnakeXY +
        // CollisionVerify order, including tail-cell vacation semantics.
        {
            let player = &mut self.players[seat_index];
            player.body.push_front(new_head);
            if player.pending_growth > 0 {
                player.pending_growth -= 1;
            } else {
                player.body.pop_back();
            }
            player.last_moved = Some(direction);
        }

        // Own body skips the just-moved head; every other snake's whole body
        // is deadly, exactly like the original's two-snake checks.
        let hit = level.is_wall(new_head.x as usize, new_head.y as usize)
            || self.players.iter().enumerate().any(|(index, player)| {
                let skip = usize::from(index == seat_index);
                player
                    .body
                    .iter()
                    .skip(skip)
                    .any(|segment| *segment == new_head)
            });
        if hit {
            let player = &mut self.players[seat_index];
            let died_at = (player.body.len() as i32 + player.pending_growth).min(MAX_SNAKE_LEN - 1);
            player.respawn_length = ssnake_respawn_length(died_at);
            player.pending_growth = 0;
            player.motion = Motion::Dying;
            self.charge(seat_index, SSNAKE_CRASH_CHIPS, SsnakeChipKind::Crash);
            return false;
        }

        if self.point == Some(new_head) {
            return self.eat_food(seat_index, &level);
        }
        false
    }

    /// Returns true when this was the arena's last food and the board
    /// reshuffled.
    fn eat_food(&mut self, seat_index: usize, level: &SsnakeLevel) -> bool {
        let mut rng = rand::thread_rng();
        // Original growth roll: random(growth_factor * random(3) + 1) + 2.
        let bound = level.growth_factor * rng.gen_range(0..3) + 1;
        let growth = rng.gen_range(0..bound) + 2;
        // Read the multiplier before anything moves; the eater is moving by
        // definition, so it is always at least 1.
        let multiplier = self.moving_snakes().max(1);
        let bonus = self.bonus_food;

        {
            let player = &mut self.players[seat_index];
            player.pending_growth = (player.pending_growth + growth).min(MAX_SNAKE_LEN - 1);
        }

        let food_chips = ssnake_food_chips(self.food_wall_edges, bonus, multiplier);
        let kind = match bonus {
            true => SsnakeChipKind::BonusFood,
            false => SsnakeChipKind::Food,
        };
        self.award(seat_index, food_chips, kind);

        self.points_left -= 1;
        if self.points_left > 0 {
            self.spawn_food();
            return false;
        }

        // The closer's food and clear bonus land on the same tick, so the
        // owner sees them as one pop: `+270 cleared` rather than two numbers
        // fighting for the same frame.
        self.award(
            seat_index,
            SSNAKE_CLEAR_CHIPS * multiplier,
            SsnakeChipKind::ArenaClear,
        );
        self.advance_arena(seat_index);
        true
    }

    /// Roll a fresh arena and re-seed every seated snake into it. There is no
    /// interval and no key to press: the board simply becomes a new one and
    /// play continues.
    fn advance_arena(&mut self, cleared_by: usize) {
        let cleared_label = SsnakeColor::for_seat(cleared_by).label();
        self.reshuffle_arena(|next| match next {
            Some(name) => format!("{cleared_label} cleared the arena — next: {name}."),
            None => format!("{cleared_label} cleared the arena."),
        });
    }

    /// Swap in a new arena and arm the hold. Every route onto a new board
    /// goes through here — a natural clear and a vote-skip must leave the
    /// table in exactly the same state, including the dropped votes.
    fn reshuffle_arena(&mut self, message: impl FnOnce(Option<&str>) -> String) {
        self.load_level();
        self.frozen_until = Some(Instant::now() + SSNAKE_ARENA_FREEZE);
        self.skip_votes = [None; MAX_SEATS];
        self.status_message = message(self.level.as_ref().map(|level| level.name.as_str()));
    }

    /// Record a skip vote and reshuffle once every seated player has one.
    /// Returns true when the arena actually changed.
    fn vote_skip(&mut self, user_id: Uuid) -> bool {
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat to vote.".to_string();
            return false;
        };
        // Nothing to skip mid-hold; the board they would be voting off is
        // already gone.
        if self.is_frozen() {
            return false;
        }
        if let Some(secs) = self.skip_cooldown_secs_left() {
            self.status_message = format!("Arena was just skipped — vote again in {secs}s.");
            return false;
        }

        self.skip_votes[index] = Some(Instant::now());
        let cast = self.live_vote_count();
        let seated = self.seated_count();
        if cast < seated {
            self.status_message = format!(
                "{} voted to skip ({cast}/{seated}).",
                SsnakeColor::for_seat(index).label()
            );
            return false;
        }

        self.skip_available_at = Some(Instant::now() + SSNAKE_SKIP_COOLDOWN);
        self.reshuffle_arena(|next| match next {
            Some(name) => format!("Table voted to skip — next: {name}."),
            None => "Table voted to skip.".to_string(),
        });
        true
    }

    fn has_live_vote(&self, seat_index: usize) -> bool {
        self.seats[seat_index].is_some()
            && self.skip_votes[seat_index].is_some_and(|cast| cast.elapsed() < SSNAKE_SKIP_VOTE_TTL)
    }

    fn live_vote_count(&self) -> usize {
        (0..MAX_SEATS)
            .filter(|seat| self.has_live_vote(*seat))
            .count()
    }

    /// Whole seconds until the arena may be skipped again, `None` when it is
    /// available now.
    fn skip_cooldown_secs_left(&self) -> Option<u64> {
        let millis = self.skip_cooldown_millis_left();
        (millis > 0).then(|| millis.div_ceil(1_000))
    }

    fn skip_cooldown_millis_left(&self) -> u64 {
        self.skip_available_at
            .and_then(|until| until.checked_duration_since(Instant::now()))
            .map(|left| left.as_millis() as u64)
            .unwrap_or(0)
    }

    fn is_frozen(&self) -> bool {
        self.frozen_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn freeze_millis_left(&self) -> u64 {
        self.frozen_until
            .and_then(|until| until.checked_duration_since(Instant::now()))
            .map(|left| left.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Install a random arena (never the one already on screen) and place
    /// every seated snake in it. Also the cold-start path.
    fn load_level(&mut self) {
        self.level = self.roll_level();
        self.point = None;
        self.bonus_food = false;
        self.points_left = self
            .level
            .as_ref()
            .map(|level| level.points_needed)
            .unwrap_or(0);

        // Clear the whole board first so each snake is placed against the
        // ones already down, not against stale positions from the old arena.
        for seat_index in 0..MAX_SEATS {
            let chips = self.players[seat_index].chips;
            let last_chip = self.players[seat_index].last_chip;
            self.players[seat_index] = PlayerState::empty();
            self.players[seat_index].chips = chips;
            self.players[seat_index].last_chip = last_chip;
        }
        for seat_index in 0..MAX_SEATS {
            if self.seats[seat_index].is_some() {
                self.spawn_snake(seat_index);
            }
        }
        self.spawn_food();
    }

    fn roll_level(&self) -> Option<Arc<SsnakeLevel>> {
        if LEVELS.is_empty() {
            return None;
        }
        let mut rng = rand::thread_rng();
        for _ in 0..LEVELS.len() * 4 {
            let candidate = &LEVELS[rng.gen_range(0..LEVELS.len())];
            let repeat = self
                .level
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, candidate));
            if !repeat {
                return Some(candidate.clone());
            }
        }
        Some(LEVELS[rng.gen_range(0..LEVELS.len())].clone())
    }

    // ── Chips ──────────────────────────────────────────────────

    fn award(&mut self, seat_index: usize, chips: i64, kind: SsnakeChipKind) {
        if chips <= 0 {
            return;
        }
        self.players[seat_index].chips += chips;
        self.players[seat_index].last_chip = Some(kind);
    }

    fn charge(&mut self, seat_index: usize, chips: i64, kind: SsnakeChipKind) {
        if chips <= 0 {
            return;
        }
        self.players[seat_index].chips -= chips;
        self.players[seat_index].last_chip = Some(kind);
    }

    // ── Placement ──────────────────────────────────────────────

    fn spawn_food(&mut self) {
        self.point = self.random_free_cell();
        self.bonus_food = rand::thread_rng().gen_range(0..SSNAKE_BONUS_FOOD_ODDS) == 0;
        self.food_wall_edges = self.point.map(|pos| self.wall_edges(pos)).unwrap_or(0);
    }

    /// Arena walls orthogonally touching a cell, 0-4. Neighbours are taken
    /// through the warp edges, the same way a snake would reach them, so a
    /// food on the border is scored by what actually surrounds it.
    fn wall_edges(&self, pos: Pos) -> i64 {
        let Some(level) = self.level.as_ref() else {
            return 0;
        };
        [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ]
        .into_iter()
        .filter(|direction| {
            let neighbour = wrap_pos(pos, *direction, level.width, level.height);
            level.is_wall(neighbour.x as usize, neighbour.y as usize)
        })
        .count() as i64
    }

    /// Drop a snake into the emptiest pocket of the arena. A joining player
    /// must not materialise inside the pack, so candidates are scored by how
    /// far the nearest rival head is and the roomiest one wins.
    fn spawn_snake(&mut self, seat_index: usize) {
        let initial_length = self
            .level
            .as_ref()
            .map(|level| level.initial_length)
            .unwrap_or(0);
        {
            let player = &mut self.players[seat_index];
            player.body.clear();
            player.pending_growth = 0;
            player.motion = Motion::Idle;
            player.last_moved = None;
        }
        if let Some(spawn) = self.spawn_cell(seat_index) {
            self.players[seat_index].body.push_back(spawn);
            self.players[seat_index].pending_growth = initial_length.min(MAX_SNAKE_LEN - 1);
        }
    }

    fn spawn_cell(&self, seat_index: usize) -> Option<Pos> {
        let level = self.level.as_ref()?;
        let heads: Vec<Pos> = self
            .players
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != seat_index)
            .filter_map(|(_, player)| player.body.front().copied())
            .collect();

        let mut rng = rand::thread_rng();
        let budget = level.width * level.height * 4;
        let mut best: Option<(Pos, u32)> = None;
        let mut sampled = 0usize;
        for _ in 0..budget {
            if sampled >= SPAWN_CANDIDATES {
                break;
            }
            let pos = Pos {
                x: rng.gen_range(0..level.width) as u16,
                y: rng.gen_range(0..level.height) as u16,
            };
            if !self.is_free(pos) {
                continue;
            }
            sampled += 1;
            let clearance = heads
                .iter()
                .map(|head| torus_distance(pos, *head, level.width, level.height))
                .min()
                .unwrap_or(u32::MAX);
            if best.is_none_or(|(_, best_clearance)| clearance > best_clearance) {
                best = Some((pos, clearance));
            }
        }
        best.map(|(pos, _)| pos)
    }

    fn random_free_cell(&self) -> Option<Pos> {
        let level = self.level.as_ref()?;
        let mut rng = rand::thread_rng();
        for _ in 0..(level.width * level.height * 4) {
            let pos = Pos {
                x: rng.gen_range(0..level.width) as u16,
                y: rng.gen_range(0..level.height) as u16,
            };
            if self.is_free(pos) {
                return Some(pos);
            }
        }
        None
    }

    fn is_free(&self, pos: Pos) -> bool {
        let Some(level) = self.level.as_ref() else {
            return false;
        };
        !level.is_wall(pos.x as usize, pos.y as usize)
            && self.point != Some(pos)
            && !self.players.iter().any(|player| player.body.contains(&pos))
    }

    // ── Housekeeping ───────────────────────────────────────────

    /// A snake that has been steered keeps slithering on its own, so the idle
    /// timer tracks keypresses, not motion: an abandoned snake still frees
    /// its seat.
    ///
    /// The kick banks the seat's tally like any other exit. Nothing frees a
    /// seat when a session drops, so this is the settle path for everyone who
    /// closes their terminal rather than pressing `l`, and skipping it would
    /// quietly void their whole visit.
    fn kick_inactive_user(&mut self, user_id: Uuid, activity_generation: u64) -> KickOutcome {
        let Some(index) = self.seat_index(user_id) else {
            return KickOutcome::default();
        };
        if self.activity_generation[index] != activity_generation {
            return KickOutcome::default();
        }
        if self.last_activity[index].elapsed() < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS) {
            return KickOutcome::default();
        }
        // Involuntary, so no bail penalty on the way out.
        let settlement = self.leave(user_id, false);
        self.status_message = "Idle snake left the arena.".to_string();
        KickOutcome {
            reclaimed: true,
            settlement,
        }
    }

    fn seated_count(&self) -> usize {
        self.seats.iter().filter(|seat| seat.is_some()).count()
    }

    fn seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|seat| *seat == Some(user_id))
    }

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let index = self.seat_index(user_id)?;
        self.last_activity[index] = Instant::now();
        self.activity_generation[index] = self.activity_generation[index].wrapping_add(1);
        Some(self.activity_generation[index])
    }
}

fn wrap_pos(head: Pos, direction: Direction, width: usize, height: usize) -> Pos {
    let (dx, dy) = direction.delta();
    let x = (head.x as i32 + dx as i32).rem_euclid(width as i32) as u16;
    let y = (head.y as i32 + dy as i32).rem_euclid(height as i32) as u16;
    Pos { x, y }
}

/// Manhattan distance across a wrapping arena: the warp edges make the board
/// a torus, so two cells hugging opposite walls are neighbours, not opposites.
fn torus_distance(a: Pos, b: Pos, width: usize, height: usize) -> u32 {
    let dx = u32::from(a.x.abs_diff(b.x));
    let dy = u32::from(a.y.abs_diff(b.y));
    dx.min(width as u32 - dx) + dy.min(height as u32 - dy)
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
