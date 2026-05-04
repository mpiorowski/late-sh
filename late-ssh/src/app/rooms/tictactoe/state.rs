use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{TicTacToeService, TicTacToeSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    X,
    O,
}

impl Mark {
    pub fn other(self) -> Self {
        match self {
            Self::X => Self::O,
            Self::O => Self::X,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::O => "O",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Winner {
    Mark(Mark),
    Draw,
}

pub struct State {
    user_id: Uuid,
    cursor: usize,
    snapshot: TicTacToeSnapshot,
    svc: TicTacToeService,
    snapshot_rx: watch::Receiver<TicTacToeSnapshot>,
}

impl State {
    pub fn new(svc: TicTacToeService, user_id: Uuid) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        Self {
            user_id,
            cursor: 0,
            snapshot,
            svc,
            snapshot_rx,
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn is_self(&self, user_id: Uuid) -> bool {
        self.user_id == user_id
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
    }

    pub fn snapshot(&self) -> &TicTacToeSnapshot {
        &self.snapshot
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn seat_index(&self) -> Option<usize> {
        self.snapshot
            .seats
            .iter()
            .position(|seat| *seat == Some(self.user_id))
    }

    pub fn user_mark(&self) -> Option<Mark> {
        match self.seat_index()? {
            0 => Some(Mark::X),
            1 => Some(Mark::O),
            _ => None,
        }
    }

    pub fn sit(&self) {
        self.svc.sit_task(self.user_id);
    }

    pub fn leave_seat(&self) {
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn place_at_cursor(&self) {
        self.svc.place_task(self.user_id, self.cursor);
    }

    pub fn reset(&self) {
        self.svc.reset_task(self.user_id);
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let row = self.cursor / 3;
        let col = self.cursor % 3;
        let next_row = (row as isize + dy).clamp(0, 2) as usize;
        let next_col = (col as isize + dx).clamp(0, 2) as usize;
        self.cursor = next_row * 3 + next_col;
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(8);
    }
}

pub fn winning_mark(board: &[Option<Mark>; 9]) -> Option<Mark> {
    const LINES: [[usize; 3]; 8] = [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8],
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8],
        [0, 4, 8],
        [2, 4, 6],
    ];
    for line in LINES {
        let Some(mark) = board[line[0]] else {
            continue;
        };
        if board[line[1]] == Some(mark) && board[line[2]] == Some(mark) {
            return Some(mark);
        }
    }
    None
}
