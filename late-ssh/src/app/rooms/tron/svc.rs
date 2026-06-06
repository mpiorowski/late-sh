use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use late_core::models::reward::tron_win_reward_key;
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{
        backend::RoomGameEvent,
        svc::RoomsService,
        tron::{
            settings::TronTableSettings,
            state::{
                BOARD_CELLS, BOARD_HEIGHT, BOARD_WIDTH, Direction, Position, SEAT_COUNT, TronColor,
                TronOutcome, TronPhase, TronPickup,
            },
        },
    },
};

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
const GAP_PERIOD: u16 = 7;
const PICKUP_COUNT: usize = 6;
const MAX_SHIELD_CHARGES: u8 = 2;
const MAX_PHASE_CHARGES: u8 = 2;
const MAX_GAP_MOVES: u8 = 6;
const PICKUP_GAP_MOVES: u8 = 3;
const TRON_WIN_LEDGER_REASON: &str = "tron_win";
pub const TRON_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(5 * 60);
pub const TRON_TWO_PLAYER_WIN_CHIPS: i64 = 50;
pub const TRON_THREE_PLAYER_WIN_CHIPS: i64 = 75;
pub const TRON_FOUR_PLAYER_WIN_CHIPS: i64 = 100;
const TRON_PLAYED_MIN_TICKS: u32 = 30;

#[derive(Clone)]
pub struct TronService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    settings: TronTableSettings,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<TronSnapshot>,
    snapshot_rx: watch::Receiver<TronSnapshot>,
    rooms_service: RoomsService,
    room_in_round: Arc<AtomicBool>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TronPlayerSnapshot {
    pub head: Option<Position>,
    pub direction: Direction,
    pub alive: bool,
    pub crashed: bool,
    pub shield_charges: u8,
    pub phase_charges: u8,
    pub gap_moves: u8,
}

impl TronPlayerSnapshot {
    const fn empty() -> Self {
        Self {
            head: None,
            direction: Direction::Right,
            alive: false,
            crashed: false,
            shield_charges: 0,
            phase_charges: 0,
            gap_moves: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TronSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; SEAT_COUNT],
    pub board: [Option<usize>; BOARD_CELLS],
    pub pickups: [Option<TronPickup>; BOARD_CELLS],
    pub players: [TronPlayerSnapshot; SEAT_COUNT],
    pub phase: TronPhase,
    pub outcome: Option<TronOutcome>,
    pub status_message: String,
    pub speed_label: String,
    pub mode_label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickLoop {
    generation: u64,
    tick_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WinEvent {
    user_id: Uuid,
    color: TronColor,
    payout: i64,
    reward_key: Option<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GameEndEvents {
    played: Vec<Uuid>,
    win: Option<WinEvent>,
}

#[derive(Clone)]
pub struct TronServiceContext {
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
    pub rooms_service: RoomsService,
}

impl TronService {
    pub fn new_with_events(
        room_id: Uuid,
        chip_svc: ChipService,
        activity: ActivityPublisher,
        settings: TronTableSettings,
        context: TronServiceContext,
    ) -> Self {
        let TronServiceContext {
            room_event_tx,
            rooms_service,
        } = context;
        let state = SharedState::new(room_id, settings);
        let initial_snapshot = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
            chip_svc,
            activity,
            settings,
            room_event_tx,
            snapshot_tx,
            snapshot_rx,
            rooms_service,
            room_in_round: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<TronSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> TronSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, seat_joined) = {
                let mut state = svc.state.lock().await;
                let seat_joined = state.sit(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, seat_joined)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
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
            let game_end = {
                let mut state = svc.state.lock().await;
                let game_end = state.leave(user_id);
                svc.publish(&state);
                game_end
            };
            svc.publish_game_end(game_end);
        });
    }

    pub fn start_round_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, tick_loop) = {
                let mut state = svc.state.lock().await;
                let tick_loop = state.start_round(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, tick_loop)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.schedule_tick_loop(tick_loop);
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

    fn schedule_tick_loop(&self, tick_loop: Option<TickLoop>) {
        let Some(tick_loop) = tick_loop else {
            return;
        };
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(tick_loop.tick_millis)).await;
                let (running, game_end) = {
                    let mut state = svc.state.lock().await;
                    let outcome = state.tick_generation(tick_loop.generation);
                    let running = state.phase == TronPhase::Running
                        && state.round_generation == tick_loop.generation;
                    if outcome.ticked {
                        svc.publish(&state);
                    }
                    (running, outcome.game_end)
                };
                svc.publish_game_end(game_end);
                if !running {
                    break;
                }
            }
        });
    }

    fn schedule_inactivity_kick(&self, user_id: Uuid, activity_generation: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)).await;
            let game_end = {
                let mut state = svc.state.lock().await;
                let outcome = state.kick_inactive_user(user_id, activity_generation);
                if outcome.changed {
                    svc.publish(&state);
                }
                outcome.game_end
            };
            svc.publish_game_end(game_end);
        });
    }

    fn publish(&self, state: &SharedState) {
        let _ = self.snapshot_tx.send(state.snapshot());
        self.sync_room_status(state.round_active());
    }

    fn sync_room_status(&self, in_round: bool) {
        self.rooms_service
            .sync_room_status_task(self.room_id, self.room_in_round.clone(), in_round);
    }

    fn publish_game_end(&self, game_end: Option<GameEndEvents>) {
        let Some(game_end) = game_end else {
            return;
        };
        for user_id in game_end.played {
            self.activity
                .game_played_task(user_id, ActivityGame::Tron, Some("round".to_string()));
        }
        self.publish_win(game_end.win);
    }

    fn publish_win(&self, win: Option<WinEvent>) {
        if let Some(win) = win {
            if win.payout > 0 {
                let chip_svc = self.chip_svc.clone();
                tokio::spawn(async move {
                    let Some(reward_key) = win.reward_key else {
                        return;
                    };
                    match chip_svc
                        .credit_cooldown_reward_template(
                            win.user_id,
                            reward_key,
                            TRON_WIN_LEDGER_REASON,
                        )
                        .await
                    {
                        Ok(payout) => {
                            if !payout.credited {
                                tracing::info!(
                                    user_id = %win.user_id,
                                    payout = payout.amount,
                                    "suppressed tron win chips due to payout cooldown"
                                );
                            }
                        }
                        Err(error) => {
                            tracing::error!(
                                ?error,
                                user_id = %win.user_id,
                                payout = win.payout,
                                "failed to credit tron win chips"
                            );
                        }
                    }
                });
            }
            self.activity.game_won_task(
                win.user_id,
                ActivityGame::Tron,
                Some(win.color.label().to_string()),
                None,
            );
        }
    }

    pub fn settings(&self) -> TronTableSettings {
        self.settings
    }
}

#[derive(Default)]
struct TickOutcome {
    ticked: bool,
    game_end: Option<GameEndEvents>,
}

#[derive(Default)]
struct ChangeOutcome {
    changed: bool,
    game_end: Option<GameEndEvents>,
}

struct SharedState {
    room_id: Uuid,
    settings: TronTableSettings,
    seats: [Option<Uuid>; SEAT_COUNT],
    last_activity: [Instant; SEAT_COUNT],
    activity_generation: [u64; SEAT_COUNT],
    board: [Option<usize>; BOARD_CELLS],
    pickups: [Option<TronPickup>; BOARD_CELLS],
    players: [TronPlayerSnapshot; SEAT_COUNT],
    pending_directions: [Direction; SEAT_COUNT],
    trail_steps: [u16; SEAT_COUNT],
    pickup_spawn_counter: u64,
    phase: TronPhase,
    outcome: Option<TronOutcome>,
    status_message: String,
    round_generation: u64,
    round_rider_count: usize,
    round_tick_count: u32,
}

impl SharedState {
    fn new(room_id: Uuid, settings: TronTableSettings) -> Self {
        let now = Instant::now();
        Self {
            room_id,
            settings,
            seats: [None; SEAT_COUNT],
            last_activity: [now; SEAT_COUNT],
            activity_generation: [0; SEAT_COUNT],
            board: [None; BOARD_CELLS],
            pickups: [None; BOARD_CELLS],
            players: [TronPlayerSnapshot::empty(); SEAT_COUNT],
            pending_directions: [Direction::Right; SEAT_COUNT],
            trail_steps: [0; SEAT_COUNT],
            pickup_spawn_counter: 0,
            phase: TronPhase::Waiting,
            outcome: None,
            status_message: "Take a seat to ride.".to_string(),
            round_generation: 0,
            round_rider_count: 0,
            round_tick_count: 0,
        }
    }

    fn snapshot(&self) -> TronSnapshot {
        TronSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            board: self.board,
            pickups: self.pickups,
            players: self.players,
            phase: self.phase,
            outcome: self.outcome,
            status_message: self.status_message.clone(),
            speed_label: self.settings.speed.label().to_string(),
            mode_label: self.settings.mode.label().to_string(),
        }
    }

    fn sit(&mut self, user_id: Uuid) -> Option<usize> {
        if self.seats.contains(&Some(user_id)) {
            return None;
        }
        if self.phase == TronPhase::Running {
            self.status_message = "Round in progress. Watch from the rail.".to_string();
            return None;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Grid is full.".to_string();
            return None;
        };
        self.seats[index] = Some(user_id);
        self.status_message = if self.seated_count() >= 2 {
            "Ready. Press n to start.".to_string()
        } else {
            "Waiting for another rider.".to_string()
        };
        Some(index)
    }

    fn leave(&mut self, user_id: Uuid) -> Option<GameEndEvents> {
        let index = self.seat_index(user_id)?;
        if self.phase == TronPhase::Running {
            if self.players[index].alive {
                self.players[index].alive = false;
                self.players[index].crashed = true;
                let game_end = self.finish_if_needed();
                self.seats[index] = None;
                self.status_message = self
                    .outcome
                    .map(|_| self.finished_status())
                    .unwrap_or_else(|| "Rider left the grid.".to_string());
                return game_end;
            }
            self.seats[index] = None;
            self.status_message = "Crashed rider left the rail.".to_string();
            return None;
        }
        self.seats[index] = None;
        self.clear_round();
        self.phase = TronPhase::Waiting;
        self.status_message = "Seat left. Grid reset.".to_string();
        None
    }

    fn start_round(&mut self, user_id: Uuid) -> Option<TickLoop> {
        if self.seat_index(user_id).is_none() {
            self.status_message = "Take a seat before starting.".to_string();
            return None;
        }
        if self.seated_count() < 2 {
            self.status_message = "Need at least two riders.".to_string();
            return None;
        }
        if self.phase == TronPhase::Running {
            self.status_message = "Round already running.".to_string();
            return None;
        }

        self.clear_round();
        self.round_generation = self.round_generation.wrapping_add(1);
        self.round_rider_count = self.seated_count();
        self.round_tick_count = 0;
        self.phase = TronPhase::Running;
        self.outcome = None;
        for seat_index in 0..SEAT_COUNT {
            if self.seats[seat_index].is_some() {
                let start = start_position(seat_index);
                let direction = start_direction(seat_index);
                self.players[seat_index] = TronPlayerSnapshot {
                    head: Some(start),
                    direction,
                    alive: true,
                    crashed: false,
                    shield_charges: 0,
                    phase_charges: 0,
                    gap_moves: 0,
                };
                self.pending_directions[seat_index] = direction;
                self.board[start.index()] = Some(seat_index);
            }
        }
        self.seed_pickups();
        self.status_message = "Ride.".to_string();
        Some(TickLoop {
            generation: self.round_generation,
            tick_millis: self.settings.speed.tick_millis(),
        })
    }

    fn steer(&mut self, user_id: Uuid, direction: Direction) {
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat to steer.".to_string();
            return;
        };
        if self.phase != TronPhase::Running || !self.players[index].alive {
            return;
        }
        if direction == self.players[index].direction.opposite() {
            return;
        }
        self.pending_directions[index] = direction;
    }

    fn tick_generation(&mut self, generation: u64) -> TickOutcome {
        if self.phase != TronPhase::Running || self.round_generation != generation {
            return TickOutcome::default();
        }
        self.round_tick_count = self.round_tick_count.saturating_add(1);

        for seat_index in 0..SEAT_COUNT {
            if self.players[seat_index].alive {
                self.players[seat_index].direction = self.pending_directions[seat_index];
            }
        }

        let mut next_positions = [None; SEAT_COUNT];
        let mut crashed = [false; SEAT_COUNT];
        let mut phased = [false; SEAT_COUNT];
        let mut shielded = [false; SEAT_COUNT];
        for seat_index in 0..SEAT_COUNT {
            let player = self.players[seat_index];
            if !player.alive {
                continue;
            }
            let Some(head) = player.head else {
                crashed[seat_index] = true;
                continue;
            };
            let (dx, dy) = player.direction.delta();
            let next_x = head.x as i16 + dx;
            let next_y = head.y as i16 + dy;
            if next_x < 0
                || next_x >= BOARD_WIDTH as i16
                || next_y < 0
                || next_y >= BOARD_HEIGHT as i16
            {
                if self.consume_shield(seat_index) {
                    shielded[seat_index] = true;
                } else {
                    crashed[seat_index] = true;
                }
                continue;
            }
            let next = Position {
                x: next_x as u8,
                y: next_y as u8,
            };
            if self.board[next.index()].is_some() {
                if self.consume_phase(seat_index) {
                    next_positions[seat_index] = Some(next);
                    phased[seat_index] = true;
                } else if self.consume_shield(seat_index) {
                    shielded[seat_index] = true;
                } else {
                    crashed[seat_index] = true;
                }
                continue;
            }
            next_positions[seat_index] = Some(next);
        }

        for left in 0..SEAT_COUNT {
            let Some(left_pos) = next_positions[left] else {
                continue;
            };
            for (right, right_pos) in next_positions.iter().enumerate().skip(left + 1) {
                if *right_pos == Some(left_pos) {
                    crashed[left] = true;
                    crashed[right] = true;
                }
            }
        }

        // A shielded rider remains on its current cell for this tick. Do not
        // let another rider phase into that live head and create overlapping
        // heads in the public snapshot.
        for mover in 0..SEAT_COUNT {
            let Some(next) = next_positions[mover] else {
                continue;
            };
            for (stationary, &stationary_shielded) in shielded.iter().enumerate() {
                if mover == stationary || !stationary_shielded {
                    continue;
                }
                if self.players[stationary].head == Some(next) {
                    crashed[mover] = true;
                }
            }
        }

        for seat_index in 0..SEAT_COUNT {
            if !self.players[seat_index].alive {
                continue;
            }
            if crashed[seat_index] {
                self.players[seat_index].alive = false;
                self.players[seat_index].crashed = true;
                continue;
            }
            if let Some(next) = next_positions[seat_index] {
                let pickup = self.pickups[next.index()].take();
                self.players[seat_index].head = Some(next);
                if self.should_leave_trail_for_move(seat_index, phased[seat_index]) {
                    self.board[next.index()] = Some(seat_index);
                }
                if let Some(pickup) = pickup {
                    self.grant_pickup(seat_index, pickup);
                    self.spawn_pickup(pickup);
                }
            }
        }

        let game_end = self.finish_if_needed();
        if self.phase == TronPhase::Running {
            self.status_message = format!("{} riders alive.", self.alive_count());
        }
        TickOutcome {
            ticked: true,
            game_end,
        }
    }

    fn kick_inactive_user(&mut self, user_id: Uuid, activity_generation: u64) -> ChangeOutcome {
        let Some(index) = self.seat_index(user_id) else {
            return ChangeOutcome::default();
        };
        if self.activity_generation[index] != activity_generation {
            return ChangeOutcome::default();
        }
        if self.last_activity[index].elapsed() < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS) {
            return ChangeOutcome::default();
        }
        if self.phase == TronPhase::Running {
            if self.players[index].alive {
                self.players[index].alive = false;
                self.players[index].crashed = true;
                let game_end = self.finish_if_needed();
                self.seats[index] = None;
                self.status_message = self
                    .outcome
                    .map(|_| self.finished_status())
                    .unwrap_or_else(|| "Idle rider left the grid.".to_string());
                return ChangeOutcome {
                    changed: true,
                    game_end,
                };
            }
            self.seats[index] = None;
            self.status_message = "Idle crashed rider left the rail.".to_string();
            return ChangeOutcome {
                changed: true,
                game_end: None,
            };
        }
        self.seats[index] = None;
        self.clear_round();
        self.phase = TronPhase::Waiting;
        self.status_message = "Idle rider left the board.".to_string();
        ChangeOutcome {
            changed: true,
            game_end: None,
        }
    }

    fn finish_if_needed(&mut self) -> Option<GameEndEvents> {
        let alive: Vec<usize> = (0..SEAT_COUNT)
            .filter(|seat_index| self.players[*seat_index].alive)
            .collect();
        if alive.len() > 1 {
            return None;
        }
        self.phase = TronPhase::Finished;
        self.round_generation = self.round_generation.wrapping_add(1);
        self.outcome = if let Some(&seat_index) = alive.first() {
            Some(TronOutcome::Winner { seat_index })
        } else {
            Some(TronOutcome::Draw)
        };
        self.status_message = self.finished_status();
        let played = if self.round_tick_count >= TRON_PLAYED_MIN_TICKS {
            self.seats.iter().filter_map(|user_id| *user_id).collect()
        } else {
            Vec::new()
        };
        let win = match self.outcome {
            Some(TronOutcome::Winner { seat_index }) => {
                let payout = tron_win_payout(self.round_rider_count);
                self.seats[seat_index].map(|user_id| WinEvent {
                    user_id,
                    color: TronColor::for_seat(seat_index),
                    payout,
                    reward_key: tron_win_reward_key(self.round_rider_count),
                })
            }
            _ => None,
        };
        Some(GameEndEvents { played, win })
    }

    fn finished_status(&self) -> String {
        match self.outcome {
            Some(TronOutcome::Winner { seat_index }) => {
                let payout = tron_win_payout(self.round_rider_count);
                format!(
                    "{} wins {} chips. Press n for another round.",
                    TronColor::for_seat(seat_index).label(),
                    payout
                )
            }
            Some(TronOutcome::Draw) => "Grid locked. Draw. Press n for another round.".to_string(),
            None => self.status_message.clone(),
        }
    }

    fn clear_round(&mut self) {
        self.board = [None; BOARD_CELLS];
        self.pickups = [None; BOARD_CELLS];
        self.players = [TronPlayerSnapshot::empty(); SEAT_COUNT];
        self.pending_directions = [Direction::Right; SEAT_COUNT];
        self.trail_steps = [0; SEAT_COUNT];
        self.pickup_spawn_counter = 0;
        self.outcome = None;
        self.round_generation = self.round_generation.wrapping_add(1);
        self.round_rider_count = 0;
        self.round_tick_count = 0;
    }

    fn seed_pickups(&mut self) {
        if !self.settings.mode.has_pickups() {
            return;
        }
        for index in 0..PICKUP_COUNT {
            self.spawn_pickup(pickup_kind_for_slot(index));
        }
    }

    fn spawn_pickup(&mut self, pickup: TronPickup) {
        if !self.settings.mode.has_pickups() {
            return;
        }
        let spawn_id = self.pickup_spawn_counter;
        self.pickup_spawn_counter = self.pickup_spawn_counter.wrapping_add(1);
        for attempt in 0..BOARD_CELLS as u64 {
            let pos = pickup_candidate(self.round_generation, spawn_id, attempt);
            if self.is_cell_available_for_pickup(pos) {
                self.pickups[pos.index()] = Some(pickup);
                return;
            }
        }
    }

    fn is_cell_available_for_pickup(&self, pos: Position) -> bool {
        self.board[pos.index()].is_none()
            && self.pickups[pos.index()].is_none()
            && self.players.iter().all(|player| player.head != Some(pos))
    }

    fn consume_shield(&mut self, seat_index: usize) -> bool {
        if self.players[seat_index].shield_charges == 0 {
            return false;
        }
        self.players[seat_index].shield_charges -= 1;
        true
    }

    fn consume_phase(&mut self, seat_index: usize) -> bool {
        if self.players[seat_index].phase_charges == 0 {
            return false;
        }
        self.players[seat_index].phase_charges -= 1;
        true
    }

    fn should_leave_trail_for_move(&mut self, seat_index: usize, phased: bool) -> bool {
        self.trail_steps[seat_index] = self.trail_steps[seat_index].wrapping_add(1);
        if phased {
            return false;
        }
        if self.players[seat_index].gap_moves > 0 {
            self.players[seat_index].gap_moves -= 1;
            return false;
        }
        !(self.settings.mode.has_gaps() && self.trail_steps[seat_index].is_multiple_of(GAP_PERIOD))
    }

    fn grant_pickup(&mut self, seat_index: usize, pickup: TronPickup) {
        match pickup {
            TronPickup::Shield => {
                self.players[seat_index].shield_charges = self.players[seat_index]
                    .shield_charges
                    .saturating_add(1)
                    .min(MAX_SHIELD_CHARGES);
            }
            TronPickup::Phase => {
                self.players[seat_index].phase_charges = self.players[seat_index]
                    .phase_charges
                    .saturating_add(1)
                    .min(MAX_PHASE_CHARGES);
            }
            TronPickup::Gap => {
                self.players[seat_index].gap_moves = self.players[seat_index]
                    .gap_moves
                    .saturating_add(PICKUP_GAP_MOVES)
                    .min(MAX_GAP_MOVES);
            }
        }
    }

    fn seated_count(&self) -> usize {
        self.seats.iter().filter(|seat| seat.is_some()).count()
    }

    fn alive_count(&self) -> usize {
        self.players.iter().filter(|player| player.alive).count()
    }

    fn seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|seat| *seat == Some(user_id))
    }

    fn round_active(&self) -> bool {
        self.phase == TronPhase::Running
    }

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let index = self.seat_index(user_id)?;
        self.last_activity[index] = Instant::now();
        self.activity_generation[index] = self.activity_generation[index].wrapping_add(1);
        Some(self.activity_generation[index])
    }
}

pub fn tron_win_payout(rider_count: usize) -> i64 {
    match rider_count {
        0 | 1 => 0,
        2 => TRON_TWO_PLAYER_WIN_CHIPS,
        3 => TRON_THREE_PLAYER_WIN_CHIPS,
        _ => TRON_FOUR_PLAYER_WIN_CHIPS,
    }
}

fn start_position(seat_index: usize) -> Position {
    match seat_index {
        0 => Position {
            x: (BOARD_WIDTH / 4) as u8,
            y: (BOARD_HEIGHT / 2) as u8,
        },
        1 => Position {
            x: (BOARD_WIDTH * 3 / 4) as u8,
            y: (BOARD_HEIGHT / 2) as u8,
        },
        2 => Position {
            x: (BOARD_WIDTH / 2) as u8,
            y: (BOARD_HEIGHT / 4) as u8,
        },
        _ => Position {
            x: (BOARD_WIDTH / 2) as u8,
            y: (BOARD_HEIGHT * 3 / 4) as u8,
        },
    }
}

fn start_direction(seat_index: usize) -> Direction {
    match seat_index {
        0 => Direction::Right,
        1 => Direction::Left,
        2 => Direction::Down,
        _ => Direction::Up,
    }
}

fn pickup_kind_for_slot(index: usize) -> TronPickup {
    match index % 6 {
        0 | 5 => TronPickup::Shield,
        1 | 3 => TronPickup::Phase,
        _ => TronPickup::Gap,
    }
}

fn pickup_candidate(round_generation: u64, spawn_id: u64, attempt: u64) -> Position {
    let mixed = mix_u64(
        round_generation
            ^ spawn_id.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ attempt.wrapping_mul(0xBF58_476D_1CE4_E5B9),
    );
    let inner_width = BOARD_WIDTH - 4;
    let inner_height = BOARD_HEIGHT - 4;
    Position {
        x: (2 + mixed as usize % inner_width) as u8,
        y: (2 + (mixed >> 32) as usize % inner_height) as u8,
    }
}

fn mix_u64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::rooms::tron::settings::TronMode;

    fn state_with_two_players() -> (SharedState, Uuid, Uuid) {
        state_with_two_players_and_settings(TronTableSettings::default())
    }

    fn state_with_two_players_and_settings(
        settings: TronTableSettings,
    ) -> (SharedState, Uuid, Uuid) {
        let mut state = SharedState::new(Uuid::now_v7(), settings);
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        state.sit(a);
        state.sit(b);
        (state, a, b)
    }

    #[test]
    fn start_requires_two_riders() {
        let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
        let user = Uuid::now_v7();
        state.sit(user);
        assert!(state.start_round(user).is_none());
        assert_eq!(state.phase, TronPhase::Waiting);
    }

    #[test]
    fn rejects_direct_reverse_turns() {
        let (mut state, user, _) = state_with_two_players();
        state.start_round(user);
        state.steer(user, Direction::Left);
        assert_eq!(state.pending_directions[0], Direction::Right);
    }

    #[test]
    fn wall_crash_can_produce_a_winner() {
        let (mut state, user, _) = state_with_two_players();
        let tick_loop = state.start_round(user).unwrap();
        state.players[0].head = Some(Position { x: 0, y: 0 });
        state.players[0].direction = Direction::Left;
        state.pending_directions[0] = Direction::Left;
        let outcome = state.tick_generation(tick_loop.generation);
        let game_end = outcome.game_end.expect("round should end");
        assert!(game_end.win.is_some());
        assert!(game_end.played.is_empty());
        assert_eq!(state.phase, TronPhase::Finished);
        assert_eq!(state.outcome, Some(TronOutcome::Winner { seat_index: 1 }));
    }

    #[test]
    fn head_on_collision_draws_when_no_riders_survive() {
        let (mut state, user, _) = state_with_two_players();
        let tick_loop = state.start_round(user).unwrap();
        state.board = [None; BOARD_CELLS];
        state.pickups = [None; BOARD_CELLS];
        state.players[0].head = Some(Position { x: 10, y: 10 });
        state.players[0].direction = Direction::Right;
        state.pending_directions[0] = Direction::Right;
        state.players[1].head = Some(Position { x: 12, y: 10 });
        state.players[1].direction = Direction::Left;
        state.pending_directions[1] = Direction::Left;
        state.board[Position { x: 10, y: 10 }.index()] = Some(0);
        state.board[Position { x: 12, y: 10 }.index()] = Some(1);
        let outcome = state.tick_generation(tick_loop.generation);
        let game_end = outcome.game_end.expect("round should end");
        assert!(game_end.win.is_none());
        assert!(game_end.played.is_empty());
        assert_eq!(state.outcome, Some(TronOutcome::Draw));
    }

    #[test]
    fn gaps_mode_skips_every_seventh_trail_cell() {
        let (mut state, user, _) = state_with_two_players_and_settings(TronTableSettings {
            speed: Default::default(),
            mode: TronMode::Gaps,
        });
        let tick_loop = state.start_round(user).unwrap();
        for _ in 0..GAP_PERIOD {
            let outcome = state.tick_generation(tick_loop.generation);
            assert!(outcome.ticked);
        }

        let gap = Position {
            x: (BOARD_WIDTH / 4) as u8 + GAP_PERIOD as u8,
            y: (BOARD_HEIGHT / 2) as u8,
        };
        assert_eq!(state.players[0].head, Some(gap));
        assert_eq!(state.board[gap.index()], None);
    }

    #[test]
    fn phase_charge_passes_through_one_trail_cell() {
        let (mut state, user, _) = state_with_two_players();
        let tick_loop = state.start_round(user).unwrap();
        state.board = [None; BOARD_CELLS];
        state.pickups = [None; BOARD_CELLS];
        state.players[0].head = Some(Position { x: 10, y: 10 });
        state.players[0].direction = Direction::Right;
        state.players[0].phase_charges = 1;
        state.pending_directions[0] = Direction::Right;
        state.players[1].head = Some(Position { x: 40, y: 10 });
        state.players[1].direction = Direction::Right;
        state.pending_directions[1] = Direction::Right;
        state.board[Position { x: 10, y: 10 }.index()] = Some(0);
        state.board[Position { x: 11, y: 10 }.index()] = Some(1);
        state.board[Position { x: 40, y: 10 }.index()] = Some(1);

        state.tick_generation(tick_loop.generation);

        let phased_cell = Position { x: 11, y: 10 };
        assert!(state.players[0].alive);
        assert_eq!(state.players[0].head, Some(phased_cell));
        assert_eq!(state.players[0].phase_charges, 0);
        assert_eq!(state.board[phased_cell.index()], Some(1));
    }

    #[test]
    fn shield_charge_absorbs_one_trail_hit_without_moving() {
        let (mut state, user, _) = state_with_two_players();
        let tick_loop = state.start_round(user).unwrap();
        state.board = [None; BOARD_CELLS];
        state.pickups = [None; BOARD_CELLS];
        let start = Position { x: 10, y: 10 };
        state.players[0].head = Some(start);
        state.players[0].direction = Direction::Right;
        state.players[0].shield_charges = 1;
        state.pending_directions[0] = Direction::Right;
        state.players[1].head = Some(Position { x: 40, y: 10 });
        state.players[1].direction = Direction::Right;
        state.pending_directions[1] = Direction::Right;
        state.board[start.index()] = Some(0);
        state.board[Position { x: 11, y: 10 }.index()] = Some(1);
        state.board[Position { x: 40, y: 10 }.index()] = Some(1);

        state.tick_generation(tick_loop.generation);

        assert!(state.players[0].alive);
        assert_eq!(state.players[0].head, Some(start));
        assert_eq!(state.players[0].shield_charges, 0);
    }

    #[test]
    fn crashed_rider_leaving_does_not_clear_running_round() {
        let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
        let crashed_user = Uuid::now_v7();
        let alive_a = Uuid::now_v7();
        let alive_b = Uuid::now_v7();
        state.sit(crashed_user);
        state.sit(alive_a);
        state.sit(alive_b);
        state.start_round(crashed_user);
        state.players[0].alive = false;
        state.players[0].crashed = true;

        let game_end = state.leave(crashed_user);

        assert!(game_end.is_none());
        assert_eq!(state.phase, TronPhase::Running);
        assert_eq!(state.seats[0], None);
        assert!(state.players[1].alive);
        assert!(state.players[2].alive);
        assert!(state.board.iter().any(Option::is_some));
    }

    #[test]
    fn inactive_crashed_rider_does_not_clear_running_round() {
        let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
        let crashed_user = Uuid::now_v7();
        let alive_a = Uuid::now_v7();
        let alive_b = Uuid::now_v7();
        state.sit(crashed_user);
        state.sit(alive_a);
        state.sit(alive_b);
        state.start_round(crashed_user);
        state.players[0].alive = false;
        state.players[0].crashed = true;
        state.last_activity[0] = Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);
        let generation = state.activity_generation[0];

        let outcome = state.kick_inactive_user(crashed_user, generation);

        assert!(outcome.changed);
        assert!(outcome.game_end.is_none());
        assert_eq!(state.phase, TronPhase::Running);
        assert_eq!(state.seats[0], None);
        assert!(state.players[1].alive);
        assert!(state.players[2].alive);
        assert!(state.board.iter().any(Option::is_some));
    }

    #[test]
    fn payout_scales_by_round_start_rider_count() {
        assert_eq!(tron_win_payout(1), 0);
        assert_eq!(tron_win_payout(2), TRON_TWO_PLAYER_WIN_CHIPS);
        assert_eq!(tron_win_payout(3), TRON_THREE_PLAYER_WIN_CHIPS);
        assert_eq!(tron_win_payout(4), TRON_FOUR_PLAYER_WIN_CHIPS);
    }

    #[test]
    fn played_event_requires_minimum_round_ticks() {
        let (mut state, user_a, user_b) = state_with_two_players();
        state.start_round(user_a);
        state.round_tick_count = TRON_PLAYED_MIN_TICKS;
        state.players[0].alive = false;
        state.players[0].crashed = true;

        let game_end = state.finish_if_needed().expect("round should end");

        assert_eq!(game_end.played, vec![user_a, user_b]);
    }
}
