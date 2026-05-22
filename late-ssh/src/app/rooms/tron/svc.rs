use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{
        backend::RoomGameEvent,
        payout::RoomWinPayoutLimiter,
        svc::GameKind,
        tron::{
            settings::TronTableSettings,
            state::{
                BOARD_CELLS, BOARD_HEIGHT, BOARD_WIDTH, Direction, Position, SEAT_COUNT, TronColor,
                TronOutcome, TronPhase,
            },
        },
    },
};

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
pub const TRON_TWO_PLAYER_WIN_CHIPS: i64 = 50;
pub const TRON_THREE_PLAYER_WIN_CHIPS: i64 = 75;
pub const TRON_FOUR_PLAYER_WIN_CHIPS: i64 = 100;

#[derive(Clone)]
pub struct TronService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    payout_limiter: RoomWinPayoutLimiter,
    settings: TronTableSettings,
    room_display_name: String,
    room_meta_label: String,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<TronSnapshot>,
    snapshot_rx: watch::Receiver<TronSnapshot>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TronPlayerSnapshot {
    pub head: Option<Position>,
    pub direction: Direction,
    pub alive: bool,
    pub crashed: bool,
}

impl TronPlayerSnapshot {
    const fn empty() -> Self {
        Self {
            head: None,
            direction: Direction::Right,
            alive: false,
            crashed: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TronSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; SEAT_COUNT],
    pub board: [Option<usize>; BOARD_CELLS],
    pub players: [TronPlayerSnapshot; SEAT_COUNT],
    pub phase: TronPhase,
    pub outcome: Option<TronOutcome>,
    pub status_message: String,
    pub speed_label: String,
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
}

#[derive(Clone)]
pub struct TronServiceContext {
    pub payout_limiter: RoomWinPayoutLimiter,
    pub room_display_name: String,
    pub room_meta_label: String,
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
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
            payout_limiter,
            room_display_name,
            room_meta_label,
            room_event_tx,
        } = context;
        let state = SharedState::new(room_id, settings);
        let initial_snapshot = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
            chip_svc,
            activity,
            payout_limiter,
            settings,
            room_display_name,
            room_meta_label,
            room_event_tx,
            snapshot_tx,
            snapshot_rx,
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
            if let Some(seat_index) = seat_joined {
                let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                    room_id: svc.room_id,
                    user_id,
                    game_kind: GameKind::Tron,
                    display_name: svc.room_display_name.clone(),
                    seat_index,
                    meta: svc.room_meta_label.clone(),
                });
            }
        });
    }

    pub fn leave_seat_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let win = {
                let mut state = svc.state.lock().await;
                let win = state.leave(user_id);
                svc.publish(&state);
                win
            };
            svc.publish_win(win);
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
                let (running, win) = {
                    let mut state = svc.state.lock().await;
                    let outcome = state.tick_generation(tick_loop.generation);
                    let running = state.phase == TronPhase::Running
                        && state.round_generation == tick_loop.generation;
                    if outcome.ticked {
                        svc.publish(&state);
                    }
                    (running, outcome.win)
                };
                svc.publish_win(win);
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
            let win = {
                let mut state = svc.state.lock().await;
                let outcome = state.kick_inactive_user(user_id, activity_generation);
                if outcome.changed {
                    svc.publish(&state);
                }
                outcome.win
            };
            svc.publish_win(win);
        });
    }

    fn publish(&self, state: &SharedState) {
        let _ = self.snapshot_tx.send(state.snapshot());
    }

    fn publish_win(&self, win: Option<WinEvent>) {
        if let Some(win) = win {
            if self.payout_limiter.allow(win.user_id, Instant::now()) {
                let chip_svc = self.chip_svc.clone();
                tokio::spawn(async move {
                    if let Err(error) = chip_svc.credit_payout(win.user_id, win.payout).await {
                        tracing::error!(
                            ?error,
                            user_id = %win.user_id,
                            payout = win.payout,
                            "failed to credit tron win chips"
                        );
                    }
                });
            } else {
                tracing::info!(
                    user_id = %win.user_id,
                    payout = win.payout,
                    "suppressed tron win chips due to payout cooldown"
                );
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
    win: Option<WinEvent>,
}

#[derive(Default)]
struct ChangeOutcome {
    changed: bool,
    win: Option<WinEvent>,
}

struct SharedState {
    room_id: Uuid,
    settings: TronTableSettings,
    seats: [Option<Uuid>; SEAT_COUNT],
    last_activity: [Instant; SEAT_COUNT],
    activity_generation: [u64; SEAT_COUNT],
    board: [Option<usize>; BOARD_CELLS],
    players: [TronPlayerSnapshot; SEAT_COUNT],
    pending_directions: [Direction; SEAT_COUNT],
    phase: TronPhase,
    outcome: Option<TronOutcome>,
    status_message: String,
    round_generation: u64,
    round_rider_count: usize,
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
            players: [TronPlayerSnapshot::empty(); SEAT_COUNT],
            pending_directions: [Direction::Right; SEAT_COUNT],
            phase: TronPhase::Waiting,
            outcome: None,
            status_message: "Take a seat to ride.".to_string(),
            round_generation: 0,
            round_rider_count: 0,
        }
    }

    fn snapshot(&self) -> TronSnapshot {
        TronSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            board: self.board,
            players: self.players,
            phase: self.phase,
            outcome: self.outcome,
            status_message: self.status_message.clone(),
            speed_label: self.settings.speed.label().to_string(),
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

    fn leave(&mut self, user_id: Uuid) -> Option<WinEvent> {
        let index = self.seat_index(user_id)?;
        self.seats[index] = None;
        if self.phase == TronPhase::Running {
            if self.players[index].alive {
                self.players[index].alive = false;
                self.players[index].crashed = true;
                let win = self.finish_if_needed();
                self.status_message = self
                    .outcome
                    .map(|_| self.finished_status())
                    .unwrap_or_else(|| "Rider left the grid.".to_string());
                return win;
            }
            self.status_message = "Crashed rider left the rail.".to_string();
            return None;
        }
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
                };
                self.pending_directions[seat_index] = direction;
                self.board[start.index()] = Some(seat_index);
            }
        }
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

        for seat_index in 0..SEAT_COUNT {
            if self.players[seat_index].alive {
                self.players[seat_index].direction = self.pending_directions[seat_index];
            }
        }

        let mut next_positions = [None; SEAT_COUNT];
        let mut crashed = [false; SEAT_COUNT];
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
                crashed[seat_index] = true;
                continue;
            }
            let next = Position {
                x: next_x as u8,
                y: next_y as u8,
            };
            if self.board[next.index()].is_some() {
                crashed[seat_index] = true;
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
                self.players[seat_index].head = Some(next);
                self.board[next.index()] = Some(seat_index);
            }
        }

        let win = self.finish_if_needed();
        if self.phase == TronPhase::Running {
            self.status_message = format!("{} riders alive.", self.alive_count());
        }
        TickOutcome { ticked: true, win }
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
        self.seats[index] = None;
        if self.phase == TronPhase::Running {
            if self.players[index].alive {
                self.players[index].alive = false;
                self.players[index].crashed = true;
                let win = self.finish_if_needed();
                self.status_message = self
                    .outcome
                    .map(|_| self.finished_status())
                    .unwrap_or_else(|| "Idle rider left the grid.".to_string());
                return ChangeOutcome { changed: true, win };
            }
            self.status_message = "Idle crashed rider left the rail.".to_string();
            return ChangeOutcome {
                changed: true,
                win: None,
            };
        }
        self.clear_round();
        self.phase = TronPhase::Waiting;
        self.status_message = "Idle rider left the board.".to_string();
        ChangeOutcome {
            changed: true,
            win: None,
        }
    }

    fn finish_if_needed(&mut self) -> Option<WinEvent> {
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
        match self.outcome {
            Some(TronOutcome::Winner { seat_index }) => {
                let payout = tron_win_payout(self.round_rider_count);
                self.seats[seat_index].map(|user_id| WinEvent {
                    user_id,
                    color: TronColor::for_seat(seat_index),
                    payout,
                })
            }
            _ => None,
        }
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
        self.players = [TronPlayerSnapshot::empty(); SEAT_COUNT];
        self.pending_directions = [Direction::Right; SEAT_COUNT];
        self.outcome = None;
        self.round_generation = self.round_generation.wrapping_add(1);
        self.round_rider_count = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_two_players() -> (SharedState, Uuid, Uuid) {
        let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
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
        assert!(outcome.win.is_some());
        assert_eq!(state.phase, TronPhase::Finished);
        assert_eq!(state.outcome, Some(TronOutcome::Winner { seat_index: 1 }));
    }

    #[test]
    fn head_on_collision_draws_when_no_riders_survive() {
        let (mut state, user, _) = state_with_two_players();
        let tick_loop = state.start_round(user).unwrap();
        state.board = [None; BOARD_CELLS];
        state.players[0].head = Some(Position { x: 10, y: 10 });
        state.players[0].direction = Direction::Right;
        state.pending_directions[0] = Direction::Right;
        state.players[1].head = Some(Position { x: 12, y: 10 });
        state.players[1].direction = Direction::Left;
        state.pending_directions[1] = Direction::Left;
        state.board[Position { x: 10, y: 10 }.index()] = Some(0);
        state.board[Position { x: 12, y: 10 }.index()] = Some(1);
        let outcome = state.tick_generation(tick_loop.generation);
        assert!(outcome.win.is_none());
        assert_eq!(state.outcome, Some(TronOutcome::Draw));
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

        let win = state.leave(crashed_user);

        assert!(win.is_none());
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
        assert!(outcome.win.is_none());
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
}
