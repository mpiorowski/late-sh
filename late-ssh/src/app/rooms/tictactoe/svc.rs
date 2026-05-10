use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    rooms::{backend::RoomGameEvent, svc::GameKind},
};

use super::state::{Mark, Winner, winning_mark};

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;

#[derive(Clone)]
pub struct TicTacToeService {
    room_id: Uuid,
    activity: ActivityPublisher,
    room_display_name: String,
    room_meta_label: String,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
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
    pub fn new(room_id: Uuid, activity: ActivityPublisher) -> Self {
        let (room_event_tx, _) = broadcast::channel::<RoomGameEvent>(16);
        Self::new_with_events(
            room_id,
            activity,
            "Tic-Tac-Toe Board".to_string(),
            String::new(),
            room_event_tx,
        )
    }

    pub fn new_with_events(
        room_id: Uuid,
        activity: ActivityPublisher,
        room_display_name: String,
        room_meta_label: String,
        room_event_tx: broadcast::Sender<RoomGameEvent>,
    ) -> Self {
        let state = SharedState::new(room_id);
        let initial_snapshot = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
            activity,
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

    pub fn subscribe_state(&self) -> watch::Receiver<TicTacToeSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> TicTacToeSnapshot {
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
                    game_kind: GameKind::TicTacToe,
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
            let mut state = svc.state.lock().await;
            state.leave(user_id);
            svc.publish(&state);
        });
    }

    pub fn place_task(&self, user_id: Uuid, index: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, winner) = {
                let mut state = svc.state.lock().await;
                let winner = state.place(user_id, index);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, winner)
            };
            if let Some((winner_user_id, mark)) = winner {
                svc.activity.game_won_task(
                    winner_user_id,
                    ActivityGame::TicTacToe,
                    Some(mark.label().to_string()),
                    None,
                );
            }
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn reset_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.reset(user_id);
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

    fn publish(&self, state: &SharedState) {
        let _ = self.snapshot_tx.send(state.snapshot());
    }
}

struct SharedState {
    room_id: Uuid,
    seats: [Option<Uuid>; 2],
    last_activity: [Instant; 2],
    activity_generation: [u64; 2],
    board: [Option<Mark>; 9],
    turn: Mark,
    winner: Option<Winner>,
    status_message: String,
}

impl SharedState {
    fn new(room_id: Uuid) -> Self {
        let now = Instant::now();
        Self {
            room_id,
            seats: [None, None],
            last_activity: [now; 2],
            activity_generation: [0; 2],
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

    fn sit(&mut self, user_id: Uuid) -> Option<usize> {
        if self.seats.contains(&Some(user_id)) {
            return None;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Table is full.".to_string();
            return None;
        };
        self.seats[index] = Some(user_id);
        self.status_message = if self.seats.iter().all(Option::is_some) {
            "Game on. X moves first.".to_string()
        } else {
            "Waiting for a second player.".to_string()
        };
        Some(index)
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

    fn place(&mut self, user_id: Uuid, index: usize) -> Option<(Uuid, Mark)> {
        if index >= self.board.len() {
            return None;
        }
        if self.winner.is_some() {
            self.status_message = "Round is over. Press n to reset.".to_string();
            return None;
        }
        if self.seats.iter().any(Option::is_none) {
            self.status_message = "Need two players before moves count.".to_string();
            return None;
        }
        let Some(seat_index) = self.seats.iter().position(|seat| *seat == Some(user_id)) else {
            self.status_message = "Sit before playing.".to_string();
            return None;
        };
        let mark = if seat_index == 0 { Mark::X } else { Mark::O };
        if mark != self.turn {
            self.status_message = format!("{} to move.", self.turn.label());
            return None;
        }
        if self.board[index].is_some() {
            self.status_message = "That square is taken.".to_string();
            return None;
        }

        self.board[index] = Some(mark);
        if let Some(winner) = winning_mark(&self.board) {
            self.winner = Some(Winner::Mark(winner));
            self.status_message = format!("{} wins. Press n for a new round.", winner.label());
            let winner_index = if winner == Mark::X { 0 } else { 1 };
            return self.seats[winner_index].map(|user_id| (user_id, winner));
        }
        if self.board.iter().all(Option::is_some) {
            self.winner = Some(Winner::Draw);
            self.status_message = "Draw. Press n for a new round.".to_string();
            return None;
        }
        self.turn = self.turn.other();
        self.status_message = format!("{} to move.", self.turn.label());
        None
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

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let seat_index = self.seats.iter().position(|seat| *seat == Some(user_id))?;
        self.last_activity[seat_index] = Instant::now();
        self.activity_generation[seat_index] = self.activity_generation[seat_index].wrapping_add(1);
        Some(self.activity_generation[seat_index])
    }

    fn kick_inactive_user(&mut self, user_id: Uuid, activity_generation: u64) -> bool {
        let Some(seat_index) = self.seats.iter().position(|seat| *seat == Some(user_id)) else {
            return false;
        };
        if self.activity_generation[seat_index] != activity_generation
            || self.last_activity[seat_index].elapsed()
                < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)
        {
            return false;
        }

        self.seats[seat_index] = None;
        self.board = [None; 9];
        self.turn = Mark::X;
        self.winner = None;
        self.status_message = format!("Seat {} idle for 5m and left. Board reset.", seat_index + 1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn seated_player_auto_leaves_after_five_minutes_idle() {
        let room_id = user_id(1);
        let player = user_id(2);
        let mut state = SharedState::new(room_id);
        state.sit(player);
        let activity_generation = state.record_activity(player).expect("player is seated");
        state.last_activity[0] = Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);

        assert!(state.kick_inactive_user(player, activity_generation));

        assert_eq!(state.seats, [None, None]);
        assert_eq!(state.board, [None; 9]);
        assert_eq!(state.turn, Mark::X);
        assert_eq!(
            state.status_message,
            "Seat 1 idle for 5m and left. Board reset."
        );
    }

    #[test]
    fn stale_activity_generation_does_not_kick_player() {
        let room_id = user_id(1);
        let player = user_id(2);
        let mut state = SharedState::new(room_id);
        state.sit(player);
        let stale_generation = state.record_activity(player).expect("player is seated");
        let current_generation = state.record_activity(player).expect("player is seated");
        state.last_activity[0] = Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);

        assert_ne!(stale_generation, current_generation);
        assert!(!state.kick_inactive_user(player, stale_generation));
        assert_eq!(state.seats[0], Some(player));
    }
}
