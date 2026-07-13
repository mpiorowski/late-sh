use std::{
    collections::VecDeque,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use late_core::models::reward::SSNAKE_WIN_REWARD_KEY;
use rand::Rng;
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{
        backend::RoomGameEvent,
        ssnake::{
            levels::{LEVELS, SsnakeLevel},
            settings::SsnakeTableSettings,
            state::{Direction, MAX_SEATS, Motion, Pos, SsnakeColor, SsnakeOutcome, SsnakePhase},
        },
        svc::RoomsService,
    },
};

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
/// 1-in-N chance a spawned point is a life point (original: `random(35)=0`).
const LIFE_POINT_ODDS: u32 = 35;
/// Original `MaxSnakePoints`; caps body length plus pending growth.
const MAX_SNAKE_LEN: i32 = 500;
const SSNAKE_PLAYED_MIN_TICKS: u32 = 40;
const SSNAKE_WIN_LEDGER_REASON: &str = "ssnake_win";
pub const SSNAKE_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(5 * 60);
pub const SSNAKE_WIN_CHIPS: i64 = 150;

#[derive(Clone)]
pub struct SsnakeService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    settings: SsnakeTableSettings,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<SsnakeSnapshot>,
    snapshot_rx: watch::Receiver<SsnakeSnapshot>,
    rooms_service: RoomsService,
    room_in_round: Arc<AtomicBool>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug)]
pub struct SsnakePlayerSnapshot {
    pub body: Vec<Pos>,
    pub motion: Motion,
    pub lives: i32,
    pub score: i64,
    pub eliminated: bool,
    /// True while this seat is part of the current match (a seat filled
    /// after the start watches without playing until the next round).
    pub in_round: bool,
}

#[derive(Clone, Debug)]
pub struct SsnakeSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; MAX_SEATS],
    /// How many of the seats this table opens (2-4, from settings).
    pub seat_limit: usize,
    pub level: Option<Arc<SsnakeLevel>>,
    /// Arena picked for the next match ("random arena" or a level name).
    pub arena_choice: String,
    pub players: [SsnakePlayerSnapshot; MAX_SEATS],
    pub point: Option<Pos>,
    pub life_point: bool,
    pub points_left: i32,
    pub phase: SsnakePhase,
    pub outcome: Option<SsnakeOutcome>,
    pub status_message: String,
    pub speed_label: String,
    pub tick_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickLoop {
    generation: u64,
    tick_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WinEvent {
    user_id: Uuid,
    color: SsnakeColor,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GameEndEvents {
    played: Vec<Uuid>,
    win: Option<WinEvent>,
}

#[derive(Clone)]
pub struct SsnakeServiceContext {
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
    pub rooms_service: RoomsService,
}

impl SsnakeService {
    pub fn new_with_events(
        room_id: Uuid,
        chip_svc: ChipService,
        activity: ActivityPublisher,
        settings: SsnakeTableSettings,
        context: SsnakeServiceContext,
    ) -> Self {
        let SsnakeServiceContext {
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

    pub fn subscribe_state(&self) -> watch::Receiver<SsnakeSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> SsnakeSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn settings(&self) -> SsnakeTableSettings {
        self.settings
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

    pub fn select_level_task(&self, user_id: Uuid, delta: isize) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.select_level(user_id, delta);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
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
                    let running = state.phase == SsnakePhase::Running
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
        self.rooms_service.sync_room_status_task(
            self.room_id,
            self.room_in_round.clone(),
            state.phase == SsnakePhase::Running,
        );
    }

    fn publish_game_end(&self, game_end: Option<GameEndEvents>) {
        let Some(game_end) = game_end else {
            return;
        };
        for user_id in game_end.played {
            self.activity.game_played_task(
                user_id,
                ActivityGame::Ssnake,
                Some("match".to_string()),
            );
        }
        if let Some(win) = game_end.win {
            let chip_svc = self.chip_svc.clone();
            tokio::spawn(async move {
                match chip_svc
                    .credit_cooldown_reward_template(
                        win.user_id,
                        SSNAKE_WIN_REWARD_KEY,
                        SSNAKE_WIN_LEDGER_REASON,
                    )
                    .await
                {
                    Ok(payout) => {
                        if !payout.credited {
                            tracing::info!(
                                user_id = %win.user_id,
                                payout = payout.amount,
                                "suppressed ssnake win chips due to payout cooldown"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            ?error,
                            user_id = %win.user_id,
                            "failed to credit ssnake win chips"
                        );
                    }
                }
            });
            self.activity.game_won_task(
                win.user_id,
                ActivityGame::Ssnake,
                Some(win.color.label().to_string()),
                None,
            );
        }
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
    lives: i32,
    score: i64,
    eliminated: bool,
    /// Participant in the current match (set at round start).
    in_round: bool,
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
            lives: 0,
            score: 0,
            eliminated: false,
            in_round: false,
            respawn_length: 0,
        }
    }

    fn snapshot(&self) -> SsnakePlayerSnapshot {
        SsnakePlayerSnapshot {
            body: self.body.iter().copied().collect(),
            motion: self.motion,
            lives: self.lives,
            score: self.score,
            eliminated: self.eliminated,
            in_round: self.in_round,
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
    /// Open seats on this table (2-4, from settings).
    seat_limit: usize,
    last_activity: [Instant; MAX_SEATS],
    activity_generation: [u64; MAX_SEATS],
    level: Option<Arc<SsnakeLevel>>,
    /// Arena choice for the next match; starts from room settings but any
    /// seated player can cycle it before a match. `None` = random.
    selected_level: Option<usize>,
    players: [PlayerState; MAX_SEATS],
    point: Option<Pos>,
    life_point: bool,
    points_left: i32,
    level_complete_by: Option<usize>,
    phase: SsnakePhase,
    outcome: Option<SsnakeOutcome>,
    status_message: String,
    round_generation: u64,
    round_tick_count: u32,
}

impl SharedState {
    fn new(room_id: Uuid, settings: SsnakeTableSettings) -> Self {
        let now = Instant::now();
        let selected_level = settings.level;
        Self {
            room_id,
            settings,
            seats: [None; MAX_SEATS],
            seat_limit: settings.seats.clamp(2, MAX_SEATS),
            last_activity: [now; MAX_SEATS],
            activity_generation: [0; MAX_SEATS],
            level: selected_level.and_then(|index| LEVELS.get(index).cloned()),
            selected_level,
            players: empty_players(),
            point: None,
            life_point: false,
            points_left: 0,
            level_complete_by: None,
            phase: SsnakePhase::Waiting,
            outcome: None,
            status_message: "Take a seat to play.".to_string(),
            round_generation: 0,
            round_tick_count: 0,
        }
    }

    fn snapshot(&self) -> SsnakeSnapshot {
        SsnakeSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            seat_limit: self.seat_limit,
            level: self.level.clone(),
            arena_choice: self.arena_choice_label(),
            players: std::array::from_fn(|index| self.players[index].snapshot()),
            point: self.point,
            life_point: self.life_point,
            points_left: self.points_left,
            phase: self.phase,
            outcome: self.outcome,
            status_message: self.status_message.clone(),
            speed_label: self.settings.speed.label().to_string(),
            tick_count: self.round_tick_count,
        }
    }

    fn arena_choice_label(&self) -> String {
        self.selected_level
            .and_then(|index| LEVELS.get(index))
            .map(|level| level.name.clone())
            .unwrap_or_else(|| "random arena".to_string())
    }

    /// Preview arena for the waiting screen: the fixed pick, or nothing for
    /// random (revealed at match start).
    fn preview_level(&self) -> Option<Arc<SsnakeLevel>> {
        self.selected_level
            .and_then(|index| LEVELS.get(index).cloned())
    }

    fn select_level(&mut self, user_id: Uuid, delta: isize) {
        if self.phase == SsnakePhase::Running {
            return;
        }
        if self.seat_index(user_id).is_none() {
            self.status_message = "Take a seat to choose the arena.".to_string();
            return;
        }
        // Cycle over: random (0), then each level (1..=len).
        let options = LEVELS.len() as isize + 1;
        let current = self
            .selected_level
            .map(|index| index as isize + 1)
            .unwrap_or(0);
        let next = (current + delta).rem_euclid(options);
        self.selected_level = (next > 0).then(|| next as usize - 1);

        // Leave any finished match on screen conceptually behind: show the
        // newly chosen arena (or the splash for random) right away.
        self.level = self.preview_level();
        self.phase = SsnakePhase::Waiting;
        self.outcome = None;
        self.players = empty_players();
        self.point = None;
        self.status_message = format!("Next arena: {}.", self.arena_choice_label());
    }

    fn sit(&mut self, user_id: Uuid) -> Option<usize> {
        if self.seats.contains(&Some(user_id)) {
            return None;
        }
        if self.phase == SsnakePhase::Running {
            self.status_message = "Match in progress. Watch from the rail.".to_string();
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
        self.status_message = if self.seated_count() >= 2 {
            "Ready. Press n to start.".to_string()
        } else {
            "Waiting for a challenger.".to_string()
        };
        Some(index)
    }

    fn leave(&mut self, user_id: Uuid) -> Option<GameEndEvents> {
        let index = self.seat_index(user_id)?;
        if self.phase == SsnakePhase::Running {
            let forfeited = !self.players[index].eliminated;
            if forfeited {
                self.players[index].eliminated = true;
                self.players[index].body.clear();
            }
            let game_end = self.finish_if_eliminated();
            self.seats[index] = None;
            self.status_message = self
                .outcome
                .map(|_| self.finished_status())
                .unwrap_or_else(|| "Snake left the arena.".to_string());
            return game_end;
        }
        self.seats[index] = None;
        self.clear_round();
        self.phase = SsnakePhase::Waiting;
        self.status_message = "Seat left. Arena reset.".to_string();
        None
    }

    fn start_round(&mut self, user_id: Uuid) -> Option<TickLoop> {
        if self.seat_index(user_id).is_none() {
            self.status_message = "Take a seat before starting.".to_string();
            return None;
        }
        if self.seated_count() < 2 {
            self.status_message = "Need at least two snakes to start.".to_string();
            return None;
        }
        if self.phase == SsnakePhase::Running {
            self.status_message = "Match already running.".to_string();
            return None;
        }
        if LEVELS.is_empty() {
            self.status_message = "No levels available.".to_string();
            return None;
        }

        self.clear_round();
        let level = self
            .selected_level
            .and_then(|index| LEVELS.get(index).cloned())
            .unwrap_or_else(|| LEVELS[rand::thread_rng().gen_range(0..LEVELS.len())].clone());
        self.round_generation = self.round_generation.wrapping_add(1);
        self.round_tick_count = 0;
        self.phase = SsnakePhase::Running;
        self.outcome = None;
        self.points_left = level.points_needed;
        self.level = Some(level.clone());

        for seat_index in 0..MAX_SEATS {
            self.players[seat_index] = PlayerState::empty();
            if self.seats[seat_index].is_none() || seat_index >= self.seat_limit {
                continue;
            }
            let spawn = self.random_free_cell();
            let player = &mut self.players[seat_index];
            player.in_round = true;
            player.lives = level.lives;
            player.pending_growth = level.initial_length.min(MAX_SNAKE_LEN - 1);
            if let Some(spawn) = spawn {
                player.body.push_back(spawn);
            } else {
                player.eliminated = true;
            }
        }
        self.spawn_point();
        self.status_message = format!("{}: steer to slither.", level.name);
        let tick_millis = self.settings.speed.scale_tick(level.tick_millis);
        Some(TickLoop {
            generation: self.round_generation,
            tick_millis,
        })
    }

    fn steer(&mut self, user_id: Uuid, direction: Direction) {
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat to steer.".to_string();
            return;
        };
        if self.phase != SsnakePhase::Running || self.players[index].eliminated {
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

    fn tick_generation(&mut self, generation: u64) -> TickOutcome {
        if self.phase != SsnakePhase::Running || self.round_generation != generation {
            return TickOutcome::default();
        }
        self.round_tick_count = self.round_tick_count.saturating_add(1);

        // The original moves and collision-checks player 1, then player 2, so
        // later snakes see earlier snakes' fresh positions. Keep that order.
        for seat_index in 0..MAX_SEATS {
            self.step_player(seat_index);
            if self.level_complete_by.is_some() {
                break;
            }
        }

        let game_end = if self.level_complete_by.is_some() {
            Some(self.finish_level_complete())
        } else {
            self.finish_if_eliminated()
        };
        TickOutcome {
            ticked: true,
            game_end,
        }
    }

    fn step_player(&mut self, seat_index: usize) {
        if !self.players[seat_index].in_round || self.players[seat_index].eliminated {
            return;
        }
        match self.players[seat_index].motion {
            Motion::Idle => {}
            Motion::Dying => self.step_death_shrink(seat_index),
            Motion::Moving(direction) => self.step_move(seat_index, direction),
        }
    }

    /// Original death animation: drop one tail segment per tick, then either
    /// respawn at the previous size or drop out when lives run dry.
    fn step_death_shrink(&mut self, seat_index: usize) {
        if self.players[seat_index].body.len() > 1 {
            self.players[seat_index].body.pop_back();
            return;
        }
        if self.players[seat_index].lives < 0 {
            self.players[seat_index].eliminated = true;
            self.players[seat_index].body.clear();
            return;
        }
        let spawn = self.random_free_cell();
        let player = &mut self.players[seat_index];
        player.body.clear();
        if let Some(spawn) = spawn {
            player.body.push_back(spawn);
        }
        player.pending_growth = player.respawn_length.min(MAX_SNAKE_LEN - 1);
        player.motion = Motion::Idle;
        player.last_moved = None;
    }

    fn step_move(&mut self, seat_index: usize, direction: Direction) {
        let Some(level) = self.level.clone() else {
            return;
        };
        let Some(&head) = self.players[seat_index].body.front() else {
            return;
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
            player.respawn_length =
                (player.body.len() as i32 + player.pending_growth).min(MAX_SNAKE_LEN - 1);
            player.pending_growth = 0;
            player.motion = Motion::Dying;
            player.lives -= 1;
            return;
        }

        if self.point == Some(new_head) {
            self.eat_point(seat_index, &level);
        }
    }

    fn eat_point(&mut self, seat_index: usize, level: &SsnakeLevel) {
        let mut rng = rand::thread_rng();
        // Original growth roll: random(growth_factor * random(3) + 1) + 2.
        let bound = level.growth_factor * rng.gen_range(0..3) + 1;
        let growth = rng.gen_range(0..bound) + 2;
        let was_life_point = self.life_point;

        {
            let player = &mut self.players[seat_index];
            player.pending_growth = (player.pending_growth + growth).min(MAX_SNAKE_LEN - 1);
        }

        self.points_left -= 1;
        if self.points_left <= 0 {
            let player = &mut self.players[seat_index];
            player.score += level.points_bonus;
            player.lives += level.lives_bonus;
            self.level_complete_by = Some(seat_index);
        }

        let player = &mut self.players[seat_index];
        if was_life_point {
            player.lives += 1;
        } else {
            player.score += (growth as i64 / 4) * rng.gen_range(0..20) + 5;
        }

        if self.level_complete_by.is_none() {
            self.spawn_point();
        } else {
            self.point = None;
        }
    }

    fn spawn_point(&mut self) {
        self.point = self.random_free_cell();
        self.life_point = rand::thread_rng().gen_range(0..LIFE_POINT_ODDS) == 0;
    }

    fn random_free_cell(&self) -> Option<Pos> {
        let level = self.level.as_ref()?;
        let mut rng = rand::thread_rng();
        for _ in 0..(level.width * level.height * 4) {
            let pos = Pos {
                x: rng.gen_range(0..level.width) as u16,
                y: rng.gen_range(0..level.height) as u16,
            };
            let blocked = level.is_wall(pos.x as usize, pos.y as usize)
                || self.point == Some(pos)
                || self
                    .players
                    .iter()
                    .any(|player| player.body.iter().any(|segment| *segment == pos));
            if !blocked {
                return Some(pos);
            }
        }
        None
    }

    fn finish_level_complete(&mut self) -> GameEndEvents {
        // Highest score among this match's participants wins; ties draw.
        let mut best: Option<(usize, i64)> = None;
        let mut tied = false;
        for (seat_index, player) in self.players.iter().enumerate() {
            if !player.in_round {
                continue;
            }
            match best {
                None => best = Some((seat_index, player.score)),
                Some((_, best_score)) if player.score > best_score => {
                    best = Some((seat_index, player.score));
                    tied = false;
                }
                Some((_, best_score)) if player.score == best_score => tied = true,
                Some(_) => {}
            }
        }
        let outcome = match best {
            Some((seat_index, _)) if !tied => SsnakeOutcome::Winner { seat_index },
            _ => SsnakeOutcome::Draw,
        };
        self.finish(outcome)
    }

    fn finish_if_eliminated(&mut self) -> Option<GameEndEvents> {
        let mut active = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| player.in_round && !player.eliminated);
        let outcome = match (active.next(), active.next()) {
            (Some(_), Some(_)) => return None,
            (Some((seat_index, _)), None) => SsnakeOutcome::Winner { seat_index },
            (None, _) => SsnakeOutcome::Draw,
        };
        Some(self.finish(outcome))
    }

    fn finish(&mut self, outcome: SsnakeOutcome) -> GameEndEvents {
        self.phase = SsnakePhase::Finished;
        self.round_generation = self.round_generation.wrapping_add(1);
        self.outcome = Some(outcome);
        self.status_message = self.finished_status();
        let played = if self.round_tick_count >= SSNAKE_PLAYED_MIN_TICKS {
            self.seats
                .iter()
                .zip(self.players.iter())
                .filter(|(_, player)| player.in_round)
                .filter_map(|(user_id, _)| *user_id)
                .collect()
        } else {
            Vec::new()
        };
        let win = match outcome {
            SsnakeOutcome::Winner { seat_index } => {
                self.seats[seat_index].map(|user_id| WinEvent {
                    user_id,
                    color: SsnakeColor::for_seat(seat_index),
                })
            }
            SsnakeOutcome::Draw => None,
        };
        GameEndEvents { played, win }
    }

    fn finished_status(&self) -> String {
        match self.outcome {
            Some(SsnakeOutcome::Winner { seat_index }) => {
                format!(
                    "{} wins {} chips. Press n for another arena.",
                    SsnakeColor::for_seat(seat_index).label(),
                    SSNAKE_WIN_CHIPS
                )
            }
            Some(SsnakeOutcome::Draw) => "Dead even. Draw. Press n for another arena.".to_string(),
            None => self.status_message.clone(),
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
        let game_end = self.leave(user_id);
        self.status_message = self
            .outcome
            .map(|_| self.finished_status())
            .unwrap_or_else(|| "Idle snake left the arena.".to_string());
        ChangeOutcome {
            changed: true,
            game_end,
        }
    }

    fn clear_round(&mut self) {
        self.level = self.preview_level();
        self.players = empty_players();
        self.point = None;
        self.life_point = false;
        self.points_left = 0;
        self.level_complete_by = None;
        self.outcome = None;
        self.round_generation = self.round_generation.wrapping_add(1);
        self.round_tick_count = 0;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::rooms::ssnake::levels::open_test_arena;

    fn state_with_two_players() -> (SharedState, Uuid, Uuid) {
        let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        state.sit(a);
        state.sit(b);
        (state, a, b)
    }

    fn started_state() -> (SharedState, Uuid, Uuid, u64) {
        let (mut state, a, b) = state_with_two_players();
        let tick_loop = state.start_round(a).expect("round should start");
        (state, a, b, tick_loop.generation)
    }

    /// Started state on a deterministic walled 30x20 arena, with both snakes
    /// parked at known safe cells and no point on the board.
    fn arena_state() -> (SharedState, Uuid, Uuid, u64) {
        let (mut state, a, b, generation) = started_state();
        state.level = Some(Arc::new(open_test_arena(30, 20)));
        state.players[0].body = VecDeque::from([Pos { x: 5, y: 5 }]);
        state.players[0].pending_growth = 0;
        state.players[1].body = VecDeque::from([Pos { x: 20, y: 10 }]);
        state.players[1].pending_growth = 0;
        state.point = None;
        (state, a, b, generation)
    }

    #[test]
    fn start_requires_two_players() {
        let mut state = SharedState::new(Uuid::now_v7(), SsnakeTableSettings::default());
        let user = Uuid::now_v7();
        state.sit(user);
        assert!(state.start_round(user).is_none());
        assert_eq!(state.phase, SsnakePhase::Waiting);
    }

    #[test]
    fn start_picks_a_level_and_seeds_players() {
        let (state, _, _, _) = started_state();
        let level = state.level.as_ref().expect("level chosen");
        assert_eq!(state.points_left, level.points_needed);
        for player in &state.players {
            assert_eq!(player.lives, level.lives);
            assert_eq!(player.body.len(), 1);
            assert_eq!(player.motion, Motion::Idle);
        }
        assert!(state.point.is_some());
    }

    #[test]
    fn snakes_hold_still_until_first_steer() {
        let (mut state, _, _, generation) = started_state();
        let heads = [state.players[0].body[0], state.players[1].body[0]];
        state.tick_generation(generation);
        assert_eq!(state.players[0].body[0], heads[0]);
        assert_eq!(state.players[1].body[0], heads[1]);
    }

    #[test]
    fn steer_rejects_reversal_against_last_move() {
        let (mut state, a, _, generation) = arena_state();
        state.steer(a, Direction::Right);
        state.tick_generation(generation);
        assert_eq!(state.players[0].last_moved, Some(Direction::Right));
        state.steer(a, Direction::Left);
        assert_eq!(state.players[0].motion, Motion::Moving(Direction::Right));
        state.steer(a, Direction::Up);
        assert_eq!(state.players[0].motion, Motion::Moving(Direction::Up));
        // Double-turn reversal within one tick is also blocked (OldDir guard).
        state.steer(a, Direction::Left);
        assert_eq!(state.players[0].motion, Motion::Moving(Direction::Up));
    }

    #[test]
    fn wall_hit_costs_a_life_and_starts_death_shrink() {
        let (mut state, a, _, generation) = arena_state();
        state.players[0].body[0] = Pos { x: 1, y: 5 };
        let lives_before = state.players[0].lives;
        state.steer(a, Direction::Left);
        state.tick_generation(generation);
        assert_eq!(state.players[0].lives, lives_before - 1);
        assert_eq!(state.players[0].motion, Motion::Dying);
    }

    #[test]
    fn last_point_awards_bonus_and_ends_match_on_score() {
        let (mut state, a, _, generation) = arena_state();
        let level = state.level.clone().unwrap();
        state.points_left = 1;
        state.life_point = false;
        state.point = Some(Pos { x: 6, y: 5 });
        state.steer(a, Direction::Right);
        let outcome = state.tick_generation(generation);
        assert_eq!(state.phase, SsnakePhase::Finished);
        assert!(state.players[0].score >= level.points_bonus);
        let game_end = outcome.game_end.expect("match should end");
        assert_eq!(
            game_end.win.map(|win| win.user_id),
            Some(a),
            "sole scorer should win"
        );
    }

    #[test]
    fn elimination_hands_the_win_to_the_survivor() {
        let (mut state, _, b, _) = started_state();
        state.players[0].eliminated = true;
        state.players[0].body.clear();
        let game_end = state.finish_if_eliminated().expect("match should end");
        assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
        assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
    }

    #[test]
    fn death_shrink_respawns_with_previous_size_while_lives_remain() {
        let (mut state, _, _, _) = started_state();
        state.players[0].motion = Motion::Dying;
        state.players[0].respawn_length = 9;
        state.players[0].lives = 0;
        state.players[0].body = VecDeque::from([Pos { x: 3, y: 3 }]);
        state.step_death_shrink(0);
        assert_eq!(state.players[0].motion, Motion::Idle);
        assert_eq!(state.players[0].pending_growth, 9);
        assert_eq!(state.players[0].body.len(), 1);
        assert!(!state.players[0].eliminated);
    }

    #[test]
    fn death_shrink_eliminates_when_out_of_lives() {
        let (mut state, _, _, _) = started_state();
        state.players[0].motion = Motion::Dying;
        state.players[0].lives = -1;
        state.players[0].body = VecDeque::from([Pos { x: 3, y: 3 }]);
        state.step_death_shrink(0);
        assert!(state.players[0].eliminated);
    }

    #[test]
    fn leaving_mid_match_forfeits_to_the_opponent() {
        let (mut state, a, b, _) = started_state();
        let game_end = state.leave(a).expect("match should end");
        assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
        assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
    }

    #[test]
    fn seated_player_cycles_arena_choice_outside_matches() {
        let (mut state, a, _) = state_with_two_players();
        assert_eq!(state.selected_level, None);
        state.select_level(a, 1);
        assert_eq!(state.selected_level, Some(0));
        assert!(state.level.is_some(), "picking a level previews it");
        state.select_level(a, -1);
        assert_eq!(state.selected_level, None);
        assert!(state.level.is_none(), "random arena shows no preview");

        // The fixed pick drives the next match.
        state.select_level(a, 3);
        assert_eq!(state.selected_level, Some(2));
        state.start_round(a).expect("round should start");
        assert_eq!(state.level.as_ref().unwrap().name, LEVELS[2].name);

        // Mid-match the choice is locked.
        state.select_level(a, 1);
        assert_eq!(state.selected_level, Some(2));
    }

    #[test]
    fn two_seat_table_rejects_a_third_snake() {
        let (mut state, _, _) = state_with_two_players();
        assert!(state.sit(Uuid::now_v7()).is_none());
    }

    #[test]
    fn three_player_match_runs_until_one_survivor() {
        let settings = SsnakeTableSettings {
            seats: 3,
            ..Default::default()
        };
        let mut state = SharedState::new(Uuid::now_v7(), settings);
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        assert_eq!(state.sit(a), Some(0));
        assert_eq!(state.sit(b), Some(1));
        assert_eq!(state.sit(c), Some(2));
        assert!(state.sit(Uuid::now_v7()).is_none(), "table caps at 3");

        state.start_round(a).expect("round should start");
        assert!(state.players[0].in_round);
        assert!(state.players[2].in_round);
        assert!(!state.players[3].in_round);

        // First knockout leaves two active snakes: the match continues.
        state.players[0].eliminated = true;
        state.players[0].body.clear();
        assert!(state.finish_if_eliminated().is_none());

        // Second knockout leaves one: the survivor wins.
        state.players[1].eliminated = true;
        state.players[1].body.clear();
        let game_end = state.finish_if_eliminated().expect("match should end");
        assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 2 }));
        assert_eq!(game_end.win.map(|win| win.user_id), Some(c));
    }

    #[test]
    fn level_complete_with_three_players_rewards_top_score() {
        let settings = SsnakeTableSettings {
            seats: 3,
            ..Default::default()
        };
        let mut state = SharedState::new(Uuid::now_v7(), settings);
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        state.sit(a);
        state.sit(b);
        state.sit(c);
        state.start_round(a).expect("round should start");
        state.players[0].score = 10;
        state.players[1].score = 30;
        state.players[2].score = 20;

        let game_end = state.finish_level_complete();

        assert_eq!(state.outcome, Some(SsnakeOutcome::Winner { seat_index: 1 }));
        assert_eq!(game_end.win.map(|win| win.user_id), Some(b));
    }

    #[test]
    fn spectators_cannot_change_the_arena() {
        let (mut state, _, _) = state_with_two_players();
        state.select_level(Uuid::now_v7(), 1);
        assert_eq!(state.selected_level, None);
    }

    #[test]
    fn wrap_pos_wraps_all_edges() {
        assert_eq!(
            wrap_pos(Pos { x: 0, y: 0 }, Direction::Left, 10, 6),
            Pos { x: 9, y: 0 }
        );
        assert_eq!(
            wrap_pos(Pos { x: 9, y: 0 }, Direction::Right, 10, 6),
            Pos { x: 0, y: 0 }
        );
        assert_eq!(
            wrap_pos(Pos { x: 0, y: 0 }, Direction::Up, 10, 6),
            Pos { x: 0, y: 5 }
        );
        assert_eq!(
            wrap_pos(Pos { x: 0, y: 5 }, Direction::Down, 10, 6),
            Pos { x: 0, y: 0 }
        );
    }

    #[test]
    fn moving_into_own_vacated_tail_cell_is_safe() {
        let (mut state, _, _, generation) = arena_state();
        // Hand-build a 2x2 loop body: head at (5,5), tail at (5,6).
        state.players[0].body = VecDeque::from([
            Pos { x: 5, y: 5 },
            Pos { x: 6, y: 5 },
            Pos { x: 6, y: 6 },
            Pos { x: 5, y: 6 },
        ]);
        state.players[0].motion = Motion::Moving(Direction::Down);
        state.players[0].last_moved = Some(Direction::Left);
        let lives_before = state.players[0].lives;
        state.tick_generation(generation);
        assert_eq!(state.players[0].lives, lives_before, "tail cell vacated");
        assert_eq!(state.players[0].motion, Motion::Moving(Direction::Down));
    }
}
