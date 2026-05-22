use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use cozy_chess::{BitBoard, Board, Color, GameStatus, Move, Piece, Square};
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
        payout::RoomWinPayoutLimiter,
        svc::GameKind,
    },
};

const MAX_SEATS: usize = 2;
const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
pub const CHESS_WIN_CHIP_PAYOUT: i64 = 500;

#[derive(Clone)]
pub struct ChessService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    payout_limiter: RoomWinPayoutLimiter,
    settings: ChessTableSettings,
    room_display_name: String,
    room_meta_label: String,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<ChessSnapshot>,
    snapshot_rx: watch::Receiver<ChessSnapshot>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug)]
pub struct ChessSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; MAX_SEATS],
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

#[derive(Clone)]
pub struct ChessServiceContext {
    pub payout_limiter: RoomWinPayoutLimiter,
    pub room_display_name: String,
    pub room_meta_label: String,
    pub room_event_tx: broadcast::Sender<RoomGameEvent>,
}

impl ChessService {
    pub fn new(room_id: Uuid, chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        let (room_event_tx, _) = broadcast::channel::<RoomGameEvent>(16);
        let settings = ChessTableSettings::default();
        Self::new_with_events(
            room_id,
            chip_svc,
            activity,
            settings,
            ChessServiceContext {
                payout_limiter: RoomWinPayoutLimiter::default(),
                room_display_name: "Chess Board".to_string(),
                room_meta_label: settings.time_control.short_label().to_string(),
                room_event_tx,
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
        let ChessServiceContext {
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

    pub fn subscribe_state(&self) -> watch::Receiver<ChessSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> ChessSnapshot {
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
                    game_kind: GameKind::Chess,
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
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.leave(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn resign_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let win = {
                let mut state = svc.state.lock().await;
                let win = state.resign(user_id);
                svc.publish(&state);
                win
            };
            svc.publish_win(win);
        });
    }

    pub fn start_game_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let deadline = {
                let mut state = svc.state.lock().await;
                let deadline = state.start_game(user_id);
                let _ = state.record_activity(user_id);
                svc.publish(&state);
                deadline
            };
            svc.schedule_deadline(deadline);
        });
    }

    pub fn move_task(&self, user_id: Uuid, from: usize, to: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, deadline, win) = {
                let mut state = svc.state.lock().await;
                let outcome = state.play_move(user_id, from, to);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, outcome.deadline, outcome.win)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.schedule_deadline(deadline);
            svc.publish_win(win);
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

    fn schedule_inactivity_kick(&self, user_id: Uuid, activity_generation: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)).await;
            let mut state = svc.state.lock().await;
            if state.kick_inactive_user(user_id, activity_generation) {
                svc.publish(&state);
            }
        });
    }

    fn schedule_deadline(&self, deadline: Option<Deadline>) {
        let Some(deadline) = deadline else {
            return;
        };
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline.at)).await;
            let win = {
                let mut state = svc.state.lock().await;
                let win = state.timeout_if_current(deadline.generation);
                if win.is_some() {
                    svc.publish(&state);
                }
                win
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
                    if let Err(error) = chip_svc
                        .credit_payout(win.user_id, CHESS_WIN_CHIP_PAYOUT)
                        .await
                    {
                        tracing::error!(
                            ?error,
                            user_id = %win.user_id,
                            "failed to credit chess win chips"
                        );
                    }
                });
            } else {
                tracing::info!(
                    user_id = %win.user_id,
                    payout = CHESS_WIN_CHIP_PAYOUT,
                    "suppressed chess win chips due to payout cooldown"
                );
            }
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
struct MoveOutcome {
    deadline: Option<Deadline>,
    win: Option<WinEvent>,
}

struct SharedState {
    room_id: Uuid,
    settings: ChessTableSettings,
    seats: [Option<Uuid>; MAX_SEATS],
    last_activity: [Instant; MAX_SEATS],
    activity_generation: [u64; MAX_SEATS],
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
}

impl SharedState {
    fn new(room_id: Uuid, settings: ChessTableSettings) -> Self {
        let now = Instant::now();
        Self {
            room_id,
            settings,
            seats: [None; MAX_SEATS],
            last_activity: [now; MAX_SEATS],
            activity_generation: [0; MAX_SEATS],
            board: Board::default(),
            phase: ChessPhase::Waiting,
            result: None,
            status_message: "Take a seat to play timed chess.".to_string(),
            clocks: initial_clocks(settings),
            active_started_at: None,
            active_deadline: None,
            deadline_generation: 0,
            last_move: None,
            move_history: Vec::new(),
        }
    }

    fn snapshot(&self) -> ChessSnapshot {
        ChessSnapshot {
            room_id: self.room_id,
            seats: self.seats,
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
        self.phase = if self.seats.iter().all(Option::is_some) {
            ChessPhase::Ready
        } else {
            ChessPhase::Waiting
        };
        self.status_message = match self.phase {
            ChessPhase::Ready => "Both players seated. Press n to start.".to_string(),
            _ => "Waiting for a second player.".to_string(),
        };
        Some(index)
    }

    fn leave(&mut self, user_id: Uuid) {
        let Some(index) = self.seat_index(user_id) else {
            return;
        };
        if self.phase == ChessPhase::Active {
            self.status_message = "Use r to resign an active game.".to_string();
            return;
        }
        self.seats[index] = None;
        self.reset_board();
        self.phase = if self.seats.iter().all(Option::is_some) {
            ChessPhase::Ready
        } else {
            ChessPhase::Waiting
        };
        self.status_message = "Seat left. Board reset.".to_string();
    }

    fn resign(&mut self, user_id: Uuid) -> Option<WinEvent> {
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
        self.user_for_color(winner).map(|user_id| WinEvent {
            user_id,
            detail: "resignation",
        })
    }

    fn start_game(&mut self, user_id: Uuid) -> Option<Deadline> {
        if self.seat_index(user_id).is_none() {
            self.status_message = "Take a seat before starting.".to_string();
            return None;
        }
        if !self.seats.iter().all(Option::is_some) {
            self.status_message = "Need both White and Black seated.".to_string();
            return None;
        }
        if self.phase == ChessPhase::Active {
            self.status_message = "Game already in progress.".to_string();
            return None;
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
        self.start_turn_clock(Instant::now())
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
        if let Some(win) = self.settle_active_clock(now) {
            return MoveOutcome {
                deadline: None,
                win: Some(win),
            };
        }

        let Some(mv) = legal_move_for(&self.board, from, to) else {
            self.status_message = "Illegal move.".to_string();
            return MoveOutcome::default();
        };

        self.board.play(mv);
        self.apply_increment(moving_color);
        let record = ChessMoveRecord {
            from,
            to,
            label: mv.to_string(),
        };
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
                    win: self.user_for_color(winner).map(|user_id| WinEvent {
                        user_id,
                        detail: "checkmate",
                    }),
                }
            }
            GameStatus::Drawn => {
                self.finish(ChessGameResult::Draw);
                self.status_message = "Game drawn.".to_string();
                MoveOutcome::default()
            }
            GameStatus::Ongoing => {
                self.status_message = self.turn_status_message();
                MoveOutcome {
                    deadline: self.start_turn_clock(now),
                    win: None,
                }
            }
        }
    }

    fn timeout_if_current(&mut self, generation: u64) -> Option<WinEvent> {
        if self.phase != ChessPhase::Active || self.deadline_generation != generation {
            return None;
        }
        self.settle_active_clock(Instant::now())
    }

    fn kick_inactive_user(&mut self, user_id: Uuid, activity_generation: u64) -> bool {
        if self.phase == ChessPhase::Active {
            return false;
        }
        let Some(index) = self.seat_index(user_id) else {
            return false;
        };
        if self.activity_generation[index] != activity_generation {
            return false;
        }
        if self.last_activity[index].elapsed() < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS) {
            return false;
        }
        self.seats[index] = None;
        self.reset_board();
        self.phase = if self.seats.iter().all(Option::is_some) {
            ChessPhase::Ready
        } else {
            ChessPhase::Waiting
        };
        self.status_message = "Idle player left the board.".to_string();
        true
    }

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let index = self.seat_index(user_id)?;
        self.last_activity[index] = Instant::now();
        self.activity_generation[index] = self.activity_generation[index].wrapping_add(1);
        Some(self.activity_generation[index])
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

    fn settle_active_clock(&mut self, now: Instant) -> Option<WinEvent> {
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

    fn finish_timeout(&mut self, loser: ChessColor) -> Option<WinEvent> {
        let winner = loser.other();
        self.finish(ChessGameResult::Timeout { winner });
        self.status_message = format!(
            "{} flagged. {} wins {} chips.",
            loser.label(),
            winner.label(),
            CHESS_WIN_CHIP_PAYOUT
        );
        self.user_for_color(winner).map(|user_id| WinEvent {
            user_id,
            detail: "timeout",
        })
    }

    fn swap_colors(&mut self) {
        self.seats.swap(0, 1);
        self.last_activity.swap(0, 1);
        self.activity_generation.swap(0, 1);
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
}
