use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use cozy_chess::{BitBoard, Board, Color, GameStatus, Move, Piece, Square, util::display_san_move};
use late_core::models::reward::CHESS_WIN_REWARD_KEY;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{
        backend::RoomGameEvent,
        chess::{
            settings::{ChessClockMode, ChessTableSettings},
            state::{
                ChessColor, ChessGameResult, ChessMoveRecord, ChessMoveSpec, ChessPhase,
                ChessPieceKind,
            },
        },
        svc::RoomsService,
    },
};

const MAX_SEATS: usize = 2;
const CHESS_WIN_LEDGER_REASON: &str = "chess_win";
pub const CHESS_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(60 * 60);
pub const CHESS_WIN_CHIP_PAYOUT: i64 = 500;
const CHESS_PLAYED_MIN_PLIES: usize = 20;
const CHESS_RUNTIME_STATE_VERSION: u8 = 1;

#[derive(Clone)]
pub struct ChessService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    settings: ChessTableSettings,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    rooms_service: RoomsService,
    room_in_round: Arc<AtomicBool>,
    snapshot_tx: watch::Sender<ChessSnapshot>,
    snapshot_rx: watch::Receiver<ChessSnapshot>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug)]
pub struct ChessSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; MAX_SEATS],
    pub ready: [bool; MAX_SEATS],
    pub pieces: [Option<ChessPiece>; 64],
    pub turn: ChessColor,
    pub phase: ChessPhase,
    pub result: Option<ChessGameResult>,
    pub status_message: String,
    pub legal_moves: Vec<ChessMoveSpec>,
    pub last_move: Option<ChessMoveRecord>,
    pub clocks: [ChessClockSnapshot; MAX_SEATS],
    pub active_deadline: Option<Instant>,
    pub time_control_label: String,
    pub in_check: bool,
    pub move_history: Vec<ChessMoveRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChessPiece {
    pub color: ChessColor,
    pub kind: ChessPieceKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChessClockSnapshot {
    pub remaining_secs: Option<u64>,
    pub move_deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClockState {
    remaining_secs: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Deadline {
    generation: u64,
    at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WinEvent {
    user_id: Uuid,
    detail: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayedEvent {
    user_id: Uuid,
    detail: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GameEndEvents {
    played: Vec<PlayedEvent>,
    win: Option<WinEvent>,
}

#[derive(Clone)]
pub struct ChessServiceContext {
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
    pub rooms_service: RoomsService,
}

impl ChessService {
    pub fn new(
        room_id: Uuid,
        chip_svc: ChipService,
        activity: ActivityPublisher,
        rooms_service: RoomsService,
    ) -> Self {
        let (room_event_tx, _) = broadcast::channel::<RoomGameEvent>(16);
        let settings = ChessTableSettings::default();
        Self::new_with_events(
            room_id,
            chip_svc,
            activity,
            settings,
            ChessServiceContext {
                room_event_tx,
                rooms_service,
            },
        )
    }

    pub fn new_with_events(
        room_id: Uuid,
        chip_svc: ChipService,
        activity: ActivityPublisher,
        settings: ChessTableSettings,
        context: ChessServiceContext,
    ) -> Self {
        Self::new_with_events_and_runtime_state(
            room_id, chip_svc, activity, settings, None, context,
        )
    }

    pub fn new_with_events_and_runtime_state(
        room_id: Uuid,
        chip_svc: ChipService,
        activity: ActivityPublisher,
        settings: ChessTableSettings,
        runtime_state: Option<&Value>,
        context: ChessServiceContext,
    ) -> Self {
        let ChessServiceContext {
            room_event_tx,
            rooms_service,
        } = context;
        let state = runtime_state
            .and_then(|value| SharedState::from_runtime_state(room_id, settings, value))
            .unwrap_or_else(|| SharedState::new(room_id, settings));
        let initial_in_round = state.round_active();
        let initial_deadline = state.current_deadline();
        let initial_snapshot = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        let svc = Self {
            room_id,
            chip_svc,
            activity,
            settings,
            room_event_tx,
            rooms_service,
            room_in_round: Arc::new(AtomicBool::new(false)),
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
        };
        svc.sync_room_status(initial_in_round);
        svc.schedule_deadline(initial_deadline);
        svc
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<ChessSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> ChessSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let seat_joined = {
                let mut state = svc.state.lock().await;
                let seat_joined = state.sit(user_id);
                svc.publish(&state);
                if seat_joined.is_some() {
                    state.bump_runtime_revision();
                    svc.persist_runtime_state(&state);
                }
                seat_joined
            };
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
            let mut state = svc.state.lock().await;
            let changed = state.leave(user_id);
            svc.publish(&state);
            if changed {
                state.bump_runtime_revision();
                svc.persist_runtime_state(&state);
            }
        });
    }

    pub fn resign_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let game_end = {
                let mut state = svc.state.lock().await;
                let game_end = state.resign(user_id);
                svc.publish(&state);
                if game_end.is_some() {
                    state.bump_runtime_revision();
                    svc.persist_runtime_state(&state);
                }
                game_end
            };
            svc.publish_game_end(game_end);
        });
    }

    pub fn start_game_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let outcome = {
                let mut state = svc.state.lock().await;
                let outcome = state.start_game(user_id);
                svc.publish(&state);
                if outcome.changed {
                    state.bump_runtime_revision();
                    svc.persist_runtime_state(&state);
                }
                outcome
            };
            svc.schedule_deadline(outcome.deadline);
        });
    }

    pub fn move_task(&self, user_id: Uuid, from: usize, to: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            let outcome = {
                let mut state = svc.state.lock().await;
                let outcome = state.play_move(user_id, from, to);
                svc.publish(&state);
                if outcome.changed {
                    state.bump_runtime_revision();
                    svc.persist_runtime_state(&state);
                }
                outcome
            };
            svc.schedule_deadline(outcome.deadline);
            svc.publish_game_end(outcome.game_end);
        });
    }

    pub fn touch_activity_task(&self, _user_id: Uuid) {
        // Chess seats are explicit reservations and remain held until the
        // player leaves the seat or resigns an active game.
    }

    fn persist_runtime_state(&self, state: &SharedState) {
        self.rooms_service
            .save_runtime_state_task(self.room_id, state.runtime_state());
    }

    fn schedule_deadline(&self, deadline: Option<Deadline>) {
        let Some(deadline) = deadline else {
            return;
        };
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline.at)).await;
            let game_end = {
                let mut state = svc.state.lock().await;
                let game_end = state.timeout_if_current(deadline.generation);
                if game_end.is_some() {
                    svc.publish(&state);
                    state.bump_runtime_revision();
                    svc.persist_runtime_state(&state);
                }
                game_end
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
        for event in game_end.played {
            self.activity.game_played_task(
                event.user_id,
                ActivityGame::Chess,
                Some(event.detail.to_string()),
            );
        }
        self.publish_win(game_end.win);
    }

    fn publish_win(&self, win: Option<WinEvent>) {
        if let Some(win) = win {
            let chip_svc = self.chip_svc.clone();
            tokio::spawn(async move {
                match chip_svc
                    .credit_cooldown_reward_template(
                        win.user_id,
                        CHESS_WIN_REWARD_KEY,
                        CHESS_WIN_LEDGER_REASON,
                    )
                    .await
                {
                    Ok(payout) => {
                        if !payout.credited {
                            tracing::info!(
                                user_id = %win.user_id,
                                payout = payout.amount,
                                "suppressed chess win chips due to payout cooldown"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            ?error,
                            user_id = %win.user_id,
                            "failed to credit chess win chips"
                        );
                    }
                }
            });
            self.activity.game_won_task(
                win.user_id,
                ActivityGame::Chess,
                Some(win.detail.to_string()),
                None,
            );
        }
    }

    pub fn settings(&self) -> ChessTableSettings {
        self.settings
    }
}

#[derive(Default)]
struct StartGameOutcome {
    deadline: Option<Deadline>,
    changed: bool,
}

#[derive(Default)]
struct MoveOutcome {
    deadline: Option<Deadline>,
    game_end: Option<GameEndEvents>,
    changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChessRuntimeClock {
    remaining_secs: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChessRuntimeState {
    version: u8,
    #[serde(default)]
    revision: u64,
    seats: [Option<Uuid>; MAX_SEATS],
    ready: [bool; MAX_SEATS],
    fen: String,
    phase: ChessPhase,
    result: Option<ChessGameResult>,
    status_message: String,
    clocks: [ChessRuntimeClock; MAX_SEATS],
    active_deadline_at: Option<DateTime<Utc>>,
    deadline_generation: u64,
    last_move: Option<ChessMoveRecord>,
    move_history: Vec<ChessMoveRecord>,
    position_history: Vec<String>,
}

struct SharedState {
    room_id: Uuid,
    settings: ChessTableSettings,
    runtime_revision: u64,
    seats: [Option<Uuid>; MAX_SEATS],
    ready: [bool; MAX_SEATS],
    board: Board,
    phase: ChessPhase,
    result: Option<ChessGameResult>,
    status_message: String,
    clocks: [ClockState; MAX_SEATS],
    active_started_at: Option<Instant>,
    active_deadline: Option<Instant>,
    deadline_generation: u64,
    last_move: Option<ChessMoveRecord>,
    move_history: Vec<ChessMoveRecord>,
    position_history: Vec<Board>,
}

impl SharedState {
    fn new(room_id: Uuid, settings: ChessTableSettings) -> Self {
        let board = Board::default();
        Self {
            room_id,
            settings,
            runtime_revision: 0,
            seats: [None; MAX_SEATS],
            ready: [false; MAX_SEATS],
            board: board.clone(),
            phase: ChessPhase::Waiting,
            result: None,
            status_message: "Take a seat to play timed chess.".to_string(),
            clocks: initial_clocks(settings),
            active_started_at: None,
            active_deadline: None,
            deadline_generation: 0,
            last_move: None,
            move_history: Vec::new(),
            position_history: vec![board],
        }
    }

    fn from_runtime_state(
        room_id: Uuid,
        settings: ChessTableSettings,
        value: &Value,
    ) -> Option<Self> {
        if value.as_object().is_some_and(serde_json::Map::is_empty) {
            return None;
        }
        let runtime: ChessRuntimeState = serde_json::from_value(value.clone()).ok()?;
        if runtime.version != CHESS_RUNTIME_STATE_VERSION {
            return None;
        }

        let board = runtime.fen.parse::<Board>().ok()?;
        let mut position_history = runtime
            .position_history
            .iter()
            .filter_map(|fen| fen.parse::<Board>().ok())
            .collect::<Vec<_>>();
        if position_history.is_empty() {
            position_history.push(board.clone());
        }

        let mut state = Self {
            room_id,
            settings,
            runtime_revision: runtime.revision,
            seats: runtime.seats,
            ready: runtime.ready,
            board,
            phase: runtime.phase,
            result: runtime.result,
            status_message: runtime.status_message,
            clocks: runtime.clocks.map(|clock| ClockState {
                remaining_secs: clock.remaining_secs,
            }),
            active_started_at: None,
            active_deadline: None,
            deadline_generation: runtime.deadline_generation,
            last_move: runtime.last_move,
            move_history: runtime.move_history,
            position_history,
        };
        state.restore_active_clock(runtime.active_deadline_at);
        Some(state)
    }

    fn runtime_state(&self) -> Value {
        json!(ChessRuntimeState {
            version: CHESS_RUNTIME_STATE_VERSION,
            revision: self.runtime_revision,
            seats: self.seats,
            ready: self.ready,
            fen: format!("{}", self.board),
            phase: self.phase,
            result: self.result,
            status_message: self.status_message.clone(),
            clocks: self.clocks.map(|clock| ChessRuntimeClock {
                remaining_secs: clock.remaining_secs,
            }),
            active_deadline_at: self.active_deadline.map(instant_as_utc),
            deadline_generation: self.deadline_generation,
            last_move: self.last_move.clone(),
            move_history: self.move_history.clone(),
            position_history: self
                .position_history
                .iter()
                .map(|board| format!("{}", board))
                .collect(),
        })
    }

    fn current_deadline(&self) -> Option<Deadline> {
        if self.phase != ChessPhase::Active {
            return None;
        }
        Some(Deadline {
            generation: self.deadline_generation,
            at: self.active_deadline?,
        })
    }

    fn round_active(&self) -> bool {
        self.phase == ChessPhase::Active
    }

    fn bump_runtime_revision(&mut self) {
        self.runtime_revision = self.runtime_revision.saturating_add(1);
    }

    fn snapshot(&self) -> ChessSnapshot {
        ChessSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            ready: self.ready,
            pieces: board_pieces(&self.board),
            turn: chess_color(self.board.side_to_move()),
            phase: self.phase,
            result: self.result,
            status_message: self.status_message.clone(),
            legal_moves: if self.phase == ChessPhase::Active {
                legal_moves(&self.board)
            } else {
                Vec::new()
            },
            last_move: self.last_move.clone(),
            clocks: self.clock_snapshots(),
            active_deadline: self.active_deadline,
            time_control_label: self.settings.time_control.short_label().to_string(),
            in_check: self.phase == ChessPhase::Active && self.board.checkers() != BitBoard::EMPTY,
            move_history: self.move_history.clone(),
        }
    }

    fn restore_active_clock(&mut self, active_deadline_at: Option<DateTime<Utc>>) {
        if self.phase != ChessPhase::Active {
            self.active_started_at = None;
            self.active_deadline = None;
            return;
        }

        let now = Instant::now();
        let Some(active_deadline_at) = active_deadline_at else {
            self.start_turn_clock(now);
            return;
        };
        let remaining = active_deadline_at
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO);
        let deadline = now + remaining;

        match self.settings.time_control.mode() {
            ChessClockMode::Countdown { .. } => {
                let active_index = chess_color(self.board.side_to_move()).seat_index();
                if remaining.is_zero() {
                    let active_remaining = self.clocks[active_index].remaining_secs.unwrap_or(0);
                    self.active_started_at = Some(
                        now.checked_sub(Duration::from_secs(active_remaining))
                            .unwrap_or(now),
                    );
                    self.active_deadline = Some(now);
                } else {
                    self.clocks[active_index].remaining_secs = Some(remaining.as_secs().max(1));
                    self.active_started_at = Some(now);
                    self.active_deadline = Some(deadline);
                }
            }
            ChessClockMode::Daily { .. } => {
                self.active_started_at = None;
                self.active_deadline = Some(deadline);
            }
        }
    }

    fn sit(&mut self, user_id: Uuid) -> Option<usize> {
        if self.seats.contains(&Some(user_id)) {
            return None;
        }
        if self.phase == ChessPhase::Active {
            self.status_message = "Game in progress. Watch from the rail.".to_string();
            return None;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Chess board is full.".to_string();
            return None;
        };
        self.seats[index] = Some(user_id);
        self.ready[index] = false;
        self.phase = if self.seats.iter().all(Option::is_some) {
            ChessPhase::Ready
        } else {
            ChessPhase::Waiting
        };
        self.status_message = match self.phase {
            ChessPhase::Ready => "Both players seated. Both press n to start.".to_string(),
            _ => "Waiting for a second player.".to_string(),
        };
        Some(index)
    }

    fn leave(&mut self, user_id: Uuid) -> bool {
        let Some(index) = self.seat_index(user_id) else {
            return false;
        };
        if self.phase == ChessPhase::Active {
            self.status_message = "Use r to resign an active game.".to_string();
            return false;
        }
        self.seats[index] = None;
        self.ready[index] = false;
        self.reset_board();
        self.phase = if self.seats.iter().all(Option::is_some) {
            ChessPhase::Ready
        } else {
            ChessPhase::Waiting
        };
        self.status_message = "Seat left. Board reset.".to_string();
        true
    }

    fn resign(&mut self, user_id: Uuid) -> Option<GameEndEvents> {
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat before resigning.".to_string();
            return None;
        };
        if self.phase != ChessPhase::Active {
            self.status_message = "No active game to resign.".to_string();
            return None;
        }
        let loser = color_for_seat(index);
        let winner = loser.other();
        self.finish(ChessGameResult::Resignation { winner });
        self.status_message = format!(
            "{} resigned. {} wins {} chips.",
            loser.label(),
            winner.label(),
            CHESS_WIN_CHIP_PAYOUT
        );
        Some(self.game_end_events("resignation", Some(winner)))
    }

    fn game_end_events(&self, detail: &'static str, winner: Option<ChessColor>) -> GameEndEvents {
        let played = if self.move_history.len() >= CHESS_PLAYED_MIN_PLIES {
            self.seats
                .iter()
                .filter_map(|user_id| user_id.map(|user_id| PlayedEvent { user_id, detail }))
                .collect()
        } else {
            Vec::new()
        };
        let win = winner.and_then(|winner| {
            self.user_for_color(winner)
                .map(|user_id| WinEvent { user_id, detail })
        });
        GameEndEvents { played, win }
    }

    fn start_game(&mut self, user_id: Uuid) -> StartGameOutcome {
        let Some(seat_index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat before starting.".to_string();
            return StartGameOutcome::default();
        };
        if !self.seats.iter().all(Option::is_some) {
            self.status_message = "Need both White and Black seated.".to_string();
            return StartGameOutcome::default();
        }
        if self.phase == ChessPhase::Active {
            self.status_message = "Game already in progress.".to_string();
            return StartGameOutcome::default();
        }
        let changed = !self.ready[seat_index];
        self.ready[seat_index] = true;
        if !self.ready.iter().all(|ready| *ready) {
            self.status_message = format!(
                "{} ready. Waiting for {} to press n.",
                color_for_seat(seat_index).label(),
                color_for_seat(waiting_ready_seat(self.ready)).label()
            );
            return StartGameOutcome {
                deadline: None,
                changed,
            };
        }
        let swapped = self.phase == ChessPhase::Finished;
        if swapped {
            self.swap_colors();
        }
        self.reset_board();
        self.phase = ChessPhase::Active;
        self.status_message = if swapped {
            "Colors swapped. White to move.".to_string()
        } else {
            "White to move.".to_string()
        };
        StartGameOutcome {
            deadline: self.start_turn_clock(Instant::now()),
            changed: true,
        }
    }

    fn play_move(&mut self, user_id: Uuid, from: usize, to: usize) -> MoveOutcome {
        let Some(seat_index) = self.seat_index(user_id) else {
            self.status_message = "Take a seat to move.".to_string();
            return MoveOutcome::default();
        };
        if self.phase != ChessPhase::Active {
            self.status_message = "Start a game before moving.".to_string();
            return MoveOutcome::default();
        }
        let moving_color = color_for_seat(seat_index);
        if chess_color(self.board.side_to_move()) != moving_color {
            self.status_message = format!(
                "{} to move.",
                chess_color(self.board.side_to_move()).label()
            );
            return MoveOutcome::default();
        }

        let now = Instant::now();
        if let Some(game_end) = self.settle_active_clock(now) {
            return MoveOutcome {
                deadline: None,
                game_end: Some(game_end),
                changed: true,
            };
        }

        let Some(mv) = legal_move_for(&self.board, from, to) else {
            self.status_message = "Illegal move.".to_string();
            return MoveOutcome::default();
        };

        let label = format!("{}", display_san_move(&self.board, mv));
        self.board.play(mv);
        self.apply_increment(moving_color);
        self.position_history.push(self.board.clone());
        let record = ChessMoveRecord { from, to, label };
        self.move_history.push(record.clone());
        self.last_move = Some(record);

        match self.board.status() {
            GameStatus::Won => {
                let winner = moving_color;
                self.finish(ChessGameResult::Checkmate { winner });
                self.status_message = format!(
                    "Checkmate. {} wins {} chips.",
                    winner.label(),
                    CHESS_WIN_CHIP_PAYOUT
                );
                MoveOutcome {
                    deadline: None,
                    game_end: Some(self.game_end_events("checkmate", Some(winner))),
                    changed: true,
                }
            }
            GameStatus::Drawn => {
                self.finish(ChessGameResult::Draw);
                self.status_message = "Game drawn.".to_string();
                MoveOutcome {
                    deadline: None,
                    game_end: Some(self.game_end_events("draw", None)),
                    changed: true,
                }
            }
            GameStatus::Ongoing => {
                if self.current_position_repetition_count() >= 3 {
                    self.finish(ChessGameResult::Draw);
                    self.status_message = "Game drawn by threefold repetition.".to_string();
                    return MoveOutcome {
                        deadline: None,
                        game_end: Some(self.game_end_events("threefold draw", None)),
                        changed: true,
                    };
                }
                self.status_message = self.turn_status_message();
                MoveOutcome {
                    deadline: self.start_turn_clock(now),
                    game_end: None,
                    changed: true,
                }
            }
        }
    }

    fn timeout_if_current(&mut self, generation: u64) -> Option<GameEndEvents> {
        if self.phase != ChessPhase::Active || self.deadline_generation != generation {
            return None;
        }
        self.settle_active_clock(Instant::now())
    }

    fn start_turn_clock(&mut self, now: Instant) -> Option<Deadline> {
        if self.phase != ChessPhase::Active {
            self.active_started_at = None;
            self.active_deadline = None;
            return None;
        }
        self.deadline_generation = self.deadline_generation.wrapping_add(1);
        self.active_started_at = Some(now);
        let deadline_at = match self.settings.time_control.mode() {
            ChessClockMode::Countdown { .. } => {
                let index = chess_color(self.board.side_to_move()).seat_index();
                now + Duration::from_secs(self.clocks[index].remaining_secs.unwrap_or(0))
            }
            ChessClockMode::Daily { move_secs } => now + Duration::from_secs(move_secs),
        };
        self.active_deadline = Some(deadline_at);
        Some(Deadline {
            generation: self.deadline_generation,
            at: deadline_at,
        })
    }

    fn settle_active_clock(&mut self, now: Instant) -> Option<GameEndEvents> {
        let active_color = chess_color(self.board.side_to_move());
        let active_index = active_color.seat_index();
        match self.settings.time_control.mode() {
            ChessClockMode::Countdown { .. } => {
                let started = self.active_started_at.unwrap_or(now);
                let elapsed_secs = now.saturating_duration_since(started).as_secs();
                let remaining = self.clocks[active_index].remaining_secs.unwrap_or(0);
                if elapsed_secs >= remaining {
                    self.clocks[active_index].remaining_secs = Some(0);
                    return self.finish_timeout(active_color);
                }
                self.clocks[active_index].remaining_secs = Some(remaining - elapsed_secs);
                self.active_started_at = Some(now);
                None
            }
            ChessClockMode::Daily { .. } => {
                if self.active_deadline.is_some_and(|deadline| now >= deadline) {
                    return self.finish_timeout(active_color);
                }
                None
            }
        }
    }

    fn finish_timeout(&mut self, loser: ChessColor) -> Option<GameEndEvents> {
        let winner = loser.other();
        self.finish(ChessGameResult::Timeout { winner });
        self.status_message = format!(
            "{} flagged. {} wins {} chips.",
            loser.label(),
            winner.label(),
            CHESS_WIN_CHIP_PAYOUT
        );
        Some(self.game_end_events("timeout", Some(winner)))
    }

    fn swap_colors(&mut self) {
        self.seats.swap(0, 1);
        self.ready.swap(0, 1);
    }

    fn apply_increment(&mut self, color: ChessColor) {
        let ChessClockMode::Countdown { increment_secs, .. } = self.settings.time_control.mode()
        else {
            return;
        };
        let index = color.seat_index();
        self.clocks[index].remaining_secs = Some(
            self.clocks[index]
                .remaining_secs
                .unwrap_or(0)
                .saturating_add(increment_secs),
        );
    }

    fn finish(&mut self, result: ChessGameResult) {
        self.phase = ChessPhase::Finished;
        self.result = Some(result);
        self.ready = [false; MAX_SEATS];
        self.active_started_at = None;
        self.active_deadline = None;
        self.deadline_generation = self.deadline_generation.wrapping_add(1);
    }

    fn reset_board(&mut self) {
        self.board = Board::default();
        self.result = None;
        self.clocks = initial_clocks(self.settings);
        self.active_started_at = None;
        self.active_deadline = None;
        self.deadline_generation = self.deadline_generation.wrapping_add(1);
        self.last_move = None;
        self.move_history.clear();
        self.ready = [false; MAX_SEATS];
        self.position_history.clear();
        self.position_history.push(self.board.clone());
    }

    fn seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|seat| *seat == Some(user_id))
    }

    fn user_for_color(&self, color: ChessColor) -> Option<Uuid> {
        self.seats[color.seat_index()]
    }

    fn clock_snapshots(&self) -> [ChessClockSnapshot; MAX_SEATS] {
        match self.settings.time_control.mode() {
            ChessClockMode::Countdown { .. } => [
                ChessClockSnapshot {
                    remaining_secs: self.clocks[0].remaining_secs,
                    move_deadline: None,
                },
                ChessClockSnapshot {
                    remaining_secs: self.clocks[1].remaining_secs,
                    move_deadline: None,
                },
            ],
            ChessClockMode::Daily { .. } => [
                ChessClockSnapshot {
                    remaining_secs: None,
                    move_deadline: (self.phase == ChessPhase::Active
                        && self.board.side_to_move() == Color::White)
                        .then_some(self.active_deadline)
                        .flatten(),
                },
                ChessClockSnapshot {
                    remaining_secs: None,
                    move_deadline: (self.phase == ChessPhase::Active
                        && self.board.side_to_move() == Color::Black)
                        .then_some(self.active_deadline)
                        .flatten(),
                },
            ],
        }
    }

    fn turn_status_message(&self) -> String {
        let color = chess_color(self.board.side_to_move());
        if self.board.checkers() != BitBoard::EMPTY {
            format!("{} to move, in check.", color.label())
        } else {
            format!("{} to move.", color.label())
        }
    }

    fn current_position_repetition_count(&self) -> usize {
        self.position_history
            .iter()
            .filter(|position| position.same_position(&self.board))
            .count()
    }
}

fn initial_clocks(settings: ChessTableSettings) -> [ClockState; MAX_SEATS] {
    match settings.time_control.mode() {
        ChessClockMode::Countdown { base_secs, .. } => {
            [ClockState {
                remaining_secs: Some(base_secs),
            }; MAX_SEATS]
        }
        ChessClockMode::Daily { .. } => {
            [ClockState {
                remaining_secs: None,
            }; MAX_SEATS]
        }
    }
}

fn board_pieces(board: &Board) -> [Option<ChessPiece>; 64] {
    std::array::from_fn(|index| {
        let square = Square::index(index);
        let piece = board.piece_on(square)?;
        let color = board.color_on(square)?;
        Some(ChessPiece {
            color: chess_color(color),
            kind: chess_piece_kind(piece),
        })
    })
}

fn legal_moves(board: &Board) -> Vec<ChessMoveSpec> {
    let mut moves = Vec::new();
    board.generate_moves(|piece_moves| {
        for mv in piece_moves {
            moves.push(ChessMoveSpec {
                from: mv.from as usize,
                to: mv.to as usize,
            });
        }
        false
    });
    moves
}

fn legal_move_for(board: &Board, from: usize, to: usize) -> Option<Move> {
    let mut fallback = None;
    let mut queen = None;
    board.generate_moves(|piece_moves| {
        for mv in piece_moves {
            if mv.from as usize == from && mv.to as usize == to {
                if mv.promotion == Some(Piece::Queen) {
                    queen = Some(mv);
                    return true;
                }
                fallback.get_or_insert(mv);
            }
        }
        false
    });
    queen.or(fallback)
}

fn instant_as_utc(instant: Instant) -> DateTime<Utc> {
    let now_instant = Instant::now();
    let now_utc = Utc::now();
    if instant >= now_instant {
        now_utc
            + chrono::Duration::from_std(instant.duration_since(now_instant)).unwrap_or_default()
    } else {
        now_utc
            - chrono::Duration::from_std(now_instant.duration_since(instant)).unwrap_or_default()
    }
}

fn chess_color(color: Color) -> ChessColor {
    match color {
        Color::White => ChessColor::White,
        Color::Black => ChessColor::Black,
    }
}

fn chess_piece_kind(piece: Piece) -> ChessPieceKind {
    match piece {
        Piece::Pawn => ChessPieceKind::Pawn,
        Piece::Knight => ChessPieceKind::Knight,
        Piece::Bishop => ChessPieceKind::Bishop,
        Piece::Rook => ChessPieceKind::Rook,
        Piece::Queen => ChessPieceKind::Queen,
        Piece::King => ChessPieceKind::King,
    }
}

fn color_for_seat(index: usize) -> ChessColor {
    match index {
        0 => ChessColor::White,
        _ => ChessColor::Black,
    }
}

fn waiting_ready_seat(ready: [bool; MAX_SEATS]) -> usize {
    ready
        .iter()
        .position(|ready| !*ready)
        .expect("ready check must only run before all seats are ready")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::rooms::chess::settings::ChessTimeControl;

    #[test]
    fn settling_countdown_clock_is_idempotent_with_repeated_checks() {
        let mut state = SharedState::new(
            Uuid::now_v7(),
            ChessTableSettings {
                time_control: ChessTimeControl::Blitz,
            },
        );
        state.phase = ChessPhase::Active;
        state.clocks[0].remaining_secs = Some(300);
        let now = Instant::now();
        state.active_started_at = Some(now - Duration::from_secs(10));

        assert!(state.settle_active_clock(now).is_none());
        assert_eq!(state.clocks[0].remaining_secs, Some(290));
        assert_eq!(state.active_started_at, Some(now));

        assert!(
            state
                .settle_active_clock(now + Duration::from_secs(5))
                .is_none()
        );
        assert_eq!(state.clocks[0].remaining_secs, Some(285));
    }

    #[test]
    fn both_seated_players_must_ready_before_clock_starts() {
        let white = Uuid::now_v7();
        let black = Uuid::now_v7();
        let mut state = SharedState::new(
            Uuid::now_v7(),
            ChessTableSettings {
                time_control: ChessTimeControl::Blitz,
            },
        );
        state.seats = [Some(white), Some(black)];
        state.phase = ChessPhase::Ready;

        let first_ready = state.start_game(black);
        assert!(first_ready.deadline.is_none());
        assert!(first_ready.changed);
        assert_eq!(state.phase, ChessPhase::Ready);
        assert_eq!(state.ready, [false, true]);
        assert_eq!(
            state.status_message,
            "Black ready. Waiting for White to press n."
        );

        let started = state.start_game(white);
        assert!(started.deadline.is_some());
        assert!(started.changed);
        assert_eq!(state.phase, ChessPhase::Active);
        assert_eq!(state.ready, [false, false]);
        assert_eq!(state.turn_status_message(), "White to move.");
    }

    #[test]
    fn move_history_labels_use_san_not_coordinate_notation() {
        let white = Uuid::now_v7();
        let black = Uuid::now_v7();
        let mut state = SharedState::new(Uuid::now_v7(), ChessTableSettings::default());
        state.seats = [Some(white), Some(black)];
        state.phase = ChessPhase::Active;

        for (user_id, from, to) in [
            (white, 12, 28), // e4
            (black, 52, 36), // e5
            (white, 1, 18),  // Nc3
            (black, 57, 42), // Nc6
        ] {
            let outcome = state.play_move(user_id, from, to);
            assert!(outcome.game_end.is_none());
        }

        let labels: Vec<&str> = state
            .move_history
            .iter()
            .map(|mv| mv.label.as_str())
            .collect();
        assert_eq!(labels, vec!["e4", "e5", "Nc3", "Nc6"]);
        assert_eq!(
            state.last_move.as_ref().map(|mv| mv.label.as_str()),
            Some("Nc6")
        );
    }

    #[test]
    fn runtime_state_restores_board_seats_and_history() {
        let room_id = Uuid::now_v7();
        let white = Uuid::now_v7();
        let black = Uuid::now_v7();
        let mut state = SharedState::new(room_id, ChessTableSettings::default());
        state.seats = [Some(white), Some(black)];
        state.phase = ChessPhase::Active;
        state.bump_runtime_revision();

        for (user_id, from, to) in [
            (white, 12, 28), // e4
            (black, 52, 36), // e5
            (white, 6, 21),  // Nf3
        ] {
            let outcome = state.play_move(user_id, from, to);
            assert!(outcome.game_end.is_none());
        }

        let runtime = state.runtime_state();
        let restored =
            SharedState::from_runtime_state(room_id, ChessTableSettings::default(), &runtime)
                .expect("runtime state restores");

        assert_eq!(format!("{}", restored.board), format!("{}", state.board));
        assert_eq!(restored.seats, [Some(white), Some(black)]);
        assert_eq!(restored.phase, ChessPhase::Active);
        assert_eq!(restored.move_history.len(), 3);
        assert_eq!(
            restored.last_move.as_ref().map(|mv| mv.label.as_str()),
            Some("Nf3")
        );
        assert_eq!(restored.runtime_revision, 1);
    }

    #[test]
    fn third_repetition_finishes_as_draw() {
        let white = Uuid::now_v7();
        let black = Uuid::now_v7();
        let mut state = SharedState::new(
            Uuid::now_v7(),
            ChessTableSettings {
                time_control: ChessTimeControl::Blitz,
            },
        );
        state.seats = [Some(white), Some(black)];
        state.phase = ChessPhase::Active;

        let cycle = [
            (white, 6, 21),  // g1f3
            (black, 62, 45), // g8f6
            (white, 21, 6),  // f3g1
            (black, 45, 62), // f6g8
        ];
        for (user_id, from, to) in cycle {
            let outcome = state.play_move(user_id, from, to);
            assert_eq!(state.phase, ChessPhase::Active);
            assert!(outcome.game_end.is_none());
        }

        for (user_id, from, to) in cycle.into_iter().take(3) {
            let outcome = state.play_move(user_id, from, to);
            assert!(outcome.game_end.is_none());
        }
        let outcome = state.play_move(black, 45, 62);
        let game_end = outcome.game_end.expect("threefold draw emits end events");
        assert_eq!(game_end.win, None);
        assert!(game_end.played.is_empty());

        assert_eq!(state.phase, ChessPhase::Finished);
        assert_eq!(state.result, Some(ChessGameResult::Draw));
        assert_eq!(state.status_message, "Game drawn by threefold repetition.");
    }
}
