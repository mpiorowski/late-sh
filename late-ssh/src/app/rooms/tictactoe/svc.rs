use std::sync::Arc;

use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use super::state::{Mark, Winner, winning_mark};

#[derive(Clone)]
pub struct TicTacToeService {
    room_id: Uuid,
    snapshot_tx: watch::Sender<TicTacToeSnapshot>,
    snapshot_rx: watch::Receiver<TicTacToeSnapshot>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug)]
pub struct TicTacToeSnapshot {
    pub room_id: Uuid,
    pub seats: [Option<Uuid>; 2],
    pub board: [Option<Mark>; 9],
    pub turn: Mark,
    pub winner: Option<Winner>,
    pub status_message: String,
}

impl TicTacToeService {
    pub fn new(room_id: Uuid) -> Self {
        let state = SharedState::new(room_id);
        let initial_snapshot = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<TicTacToeSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> TicTacToeSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.sit(user_id);
            svc.publish(&state);
        });
    }

    pub fn leave_seat_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.leave(user_id);
            svc.publish(&state);
        });
    }

    pub fn place_task(&self, user_id: Uuid, index: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.place(user_id, index);
            svc.publish(&state);
        });
    }

    pub fn reset_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.reset(user_id);
            svc.publish(&state);
        });
    }

    fn publish(&self, state: &SharedState) {
        let _ = self.snapshot_tx.send(state.snapshot());
    }
}

struct SharedState {
    room_id: Uuid,
    seats: [Option<Uuid>; 2],
    board: [Option<Mark>; 9],
    turn: Mark,
    winner: Option<Winner>,
    status_message: String,
}

impl SharedState {
    fn new(room_id: Uuid) -> Self {
        Self {
            room_id,
            seats: [None, None],
            board: [None; 9],
            turn: Mark::X,
            winner: None,
            status_message: "Take a seat to play.".to_string(),
        }
    }

    fn snapshot(&self) -> TicTacToeSnapshot {
        TicTacToeSnapshot {
            room_id: self.room_id,
            seats: self.seats,
            board: self.board,
            turn: self.turn,
            winner: self.winner,
            status_message: self.status_message.clone(),
        }
    }

    fn sit(&mut self, user_id: Uuid) {
        if self.seats.contains(&Some(user_id)) {
            return;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Table is full.".to_string();
            return;
        };
        self.seats[index] = Some(user_id);
        self.status_message = if self.seats.iter().all(Option::is_some) {
            "Game on. X moves first.".to_string()
        } else {
            "Waiting for a second player.".to_string()
        };
    }

    fn leave(&mut self, user_id: Uuid) {
        let Some(index) = self.seats.iter().position(|seat| *seat == Some(user_id)) else {
            return;
        };
        self.seats[index] = None;
        self.board = [None; 9];
        self.turn = Mark::X;
        self.winner = None;
        self.status_message = "Player left. Board reset.".to_string();
    }

    fn place(&mut self, user_id: Uuid, index: usize) {
        if index >= self.board.len() {
            return;
        }
        if self.winner.is_some() {
            self.status_message = "Round is over. Press n to reset.".to_string();
            return;
        }
        if self.seats.iter().any(Option::is_none) {
            self.status_message = "Need two players before moves count.".to_string();
            return;
        }
        let Some(seat_index) = self.seats.iter().position(|seat| *seat == Some(user_id)) else {
            self.status_message = "Sit before playing.".to_string();
            return;
        };
        let mark = if seat_index == 0 { Mark::X } else { Mark::O };
        if mark != self.turn {
            self.status_message = format!("{} to move.", self.turn.label());
            return;
        }
        if self.board[index].is_some() {
            self.status_message = "That square is taken.".to_string();
            return;
        }

        self.board[index] = Some(mark);
        if let Some(winner) = winning_mark(&self.board) {
            self.winner = Some(Winner::Mark(winner));
            self.status_message = format!("{} wins. Press n for a new round.", winner.label());
            return;
        }
        if self.board.iter().all(Option::is_some) {
            self.winner = Some(Winner::Draw);
            self.status_message = "Draw. Press n for a new round.".to_string();
            return;
        }
        self.turn = self.turn.other();
        self.status_message = format!("{} to move.", self.turn.label());
    }

    fn reset(&mut self, user_id: Uuid) {
        if !self.seats.contains(&Some(user_id)) {
            self.status_message = "Sit before resetting the board.".to_string();
            return;
        }
        self.board = [None; 9];
        self.turn = Mark::X;
        self.winner = None;
        self.status_message = "New round. X moves first.".to_string();
    }
}
