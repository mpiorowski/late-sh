use late_core::models::tetris::{Game, GameParams};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::svc::TetrisService;

pub const BOARD_WIDTH: usize = 10;
pub const BOARD_HEIGHT: usize = 20;

pub type Board = [[Option<PieceKind>; BOARD_WIDTH]; BOARD_HEIGHT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceKind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl PieceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::I => "I",
            Self::O => "O",
            Self::T => "T",
            Self::S => "S",
            Self::Z => "Z",
            Self::J => "J",
            Self::L => "L",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "O" => Self::O,
            "T" => Self::T,
            "S" => Self::S,
            "Z" => Self::Z,
            "J" => Self::J,
            "L" => Self::L,
            _ => Self::I,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivePiece {
    pub kind: PieceKind,
    pub rotation: usize,
    pub row: i32,
    pub col: i32,
}

pub struct State {
    pub user_id: Uuid,
    pub board: Board,
    pub current: ActivePiece,
    pub next: PieceKind,
    pub score: i32,
    pub best_score: i32,
    pub lines: u32,
    pub level: u32,
    pub is_game_over: bool,
    pub is_paused: bool,
    pub svc: TetrisService,
    fall_ticks: u32,
    rng: OsRng,
    bag: Vec<PieceKind>,
}

impl State {
    pub fn new(user_id: Uuid, svc: TetrisService, best_score: i32) -> Self {
        let mut state = Self {
            user_id,
            board: [[None; BOARD_WIDTH]; BOARD_HEIGHT],
            current: ActivePiece {
                kind: PieceKind::I,
                rotation: 0,
                row: 0,
                col: 3,
            },
            next: PieceKind::O,
            score: 0,
            best_score,
            lines: 0,
            level: 1,
            is_game_over: false,
            is_paused: false,
            svc,
            fall_ticks: 0,
            rng: OsRng,
            bag: Vec::new(),
        };
        let first = state.draw_from_bag();
        state.next = state.draw_from_bag();
        state.current = spawn_piece(first);
        if state.collides(state.current) {
            state.is_game_over = true;
        }
        state.persist_progress();
        state
    }

    pub fn restore(user_id: Uuid, svc: TetrisService, best_score: i32, game: Game) -> Self {
        let board =
            serde_json::from_value(game.board).unwrap_or([[None; BOARD_WIDTH]; BOARD_HEIGHT]);
        let current = ActivePiece {
            kind: PieceKind::from_name(&game.current_kind),
            rotation: game.current_rotation.max(0) as usize % 4,
            row: game.current_row,
            col: game.current_col,
        };

        Self {
            user_id,
            board,
            current,
            next: PieceKind::from_name(&game.next_kind),
            score: game.score,
            best_score: best_score.max(game.score),
            lines: game.lines.max(0) as u32,
            level: game.level.max(1) as u32,
            is_game_over: game.is_game_over,
            is_paused: false,
            svc,
            fall_ticks: 0,
            rng: OsRng,
            bag: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        let user_id = self.user_id;
        let svc = self.svc.clone();
        let best_score = self.best_score;
        *self = Self::new(user_id, svc, best_score);
    }

    pub fn tick(&mut self) -> bool {
        if self.is_game_over || self.is_paused {
            return false;
        }

        self.fall_ticks = self.fall_ticks.saturating_add(1);
        if self.fall_ticks < self.gravity_ticks() {
            return false;
        }

        self.fall_ticks = 0;
        self.step_down(false)
    }

    pub fn move_left(&mut self) -> bool {
        self.try_shift(0, -1)
    }

    pub fn move_right(&mut self) -> bool {
        self.try_shift(0, 1)
    }

    pub fn soft_drop(&mut self) -> bool {
        if self.step_down(true) {
            self.add_score(1);
            self.persist_progress();
            true
        } else {
            false
        }
    }

    pub fn rotate_cw(&mut self) -> bool {
        if self.is_game_over || self.is_paused {
            return false;
        }

        let mut rotated = self.current;
        rotated.rotation = (rotated.rotation + 1) % 4;

        for kick in [0, -1, 1, -2, 2] {
            let candidate = ActivePiece {
                col: rotated.col + kick,
                ..rotated
            };
            if !self.collides(candidate) {
                self.current = candidate;
                self.persist_progress();
                return true;
            }
        }

        false
    }

    pub fn hard_drop(&mut self) -> bool {
        if self.is_game_over || self.is_paused {
            return false;
        }

        let mut dropped = 0;
        while self.step_down(true) {
            dropped += 1;
        }
        if dropped > 0 {
            self.add_score(dropped * 2);
            self.submit_score();
            self.persist_progress();
            return true;
        }
        false
    }

    pub fn toggle_pause(&mut self) {
        if !self.is_game_over {
            self.is_paused = !self.is_paused;
            self.persist_progress();
        }
    }

    pub fn board_with_active_piece(&self) -> Board {
        let mut board = self.board;
        if !self.is_game_over {
            for (row, col) in piece_cells(self.current) {
                if row >= 0 && row < BOARD_HEIGHT as i32 && col >= 0 && col < BOARD_WIDTH as i32 {
                    board[row as usize][col as usize] = Some(self.current.kind);
                }
            }
        }
        board
    }

    pub fn gravity_ticks(&self) -> u32 {
        let level = self.level.saturating_sub(1);
        12u32.saturating_sub(level.min(9)).max(2)
    }

    fn try_shift(&mut self, dr: i32, dc: i32) -> bool {
        if self.is_game_over || self.is_paused {
            return false;
        }

        let candidate = ActivePiece {
            row: self.current.row + dr,
            col: self.current.col + dc,
            ..self.current
        };

        if self.collides(candidate) {
            return false;
        }

        self.current = candidate;
        self.persist_progress();
        true
    }

    fn step_down(&mut self, manual: bool) -> bool {
        if self.is_game_over || self.is_paused {
            return false;
        }

        let candidate = ActivePiece {
            row: self.current.row + 1,
            ..self.current
        };

        if !self.collides(candidate) {
            self.current = candidate;
            if manual {
                self.fall_ticks = 0;
            }
            return true;
        }

        self.lock_piece();
        false
    }

    fn lock_piece(&mut self) {
        for (row, col) in piece_cells(self.current) {
            if row < 0 {
                self.is_game_over = true;
                self.submit_score();
                self.persist_progress();
                return;
            }
            if row >= 0 && row < BOARD_HEIGHT as i32 && col >= 0 && col < BOARD_WIDTH as i32 {
                self.board[row as usize][col as usize] = Some(self.current.kind);
            }
        }

        let cleared = self.clear_lines();
        if cleared > 0 {
            self.lines += cleared;
            self.level = (self.lines / 10) + 1;
            self.add_score(line_clear_score(cleared, self.level));
        }

        self.current = spawn_piece(self.next);
        self.next = self.draw_from_bag();
        self.fall_ticks = 0;

        if self.collides(self.current) {
            self.is_game_over = true;
        }

        self.submit_score();
        self.persist_progress();
    }

    fn clear_lines(&mut self) -> u32 {
        let mut new_board = [[None; BOARD_WIDTH]; BOARD_HEIGHT];
        let mut write_row = BOARD_HEIGHT as i32 - 1;
        let mut cleared = 0;

        for row in (0..BOARD_HEIGHT).rev() {
            let full = self.board[row].iter().all(Option::is_some);
            if full {
                cleared += 1;
            } else {
                new_board[write_row as usize] = self.board[row];
                write_row -= 1;
            }
        }

        self.board = new_board;
        cleared
    }

    fn collides(&self, piece: ActivePiece) -> bool {
        for (row, col) in piece_cells(piece) {
            if !(0..BOARD_WIDTH as i32).contains(&col) || row >= BOARD_HEIGHT as i32 {
                return true;
            }
            if row >= 0 && self.board[row as usize][col as usize].is_some() {
                return true;
            }
        }
        false
    }

    fn draw_from_bag(&mut self) -> PieceKind {
        if self.bag.is_empty() {
            self.refill_bag();
        }
        self.bag.pop().unwrap_or(PieceKind::I)
    }

    fn refill_bag(&mut self) {
        self.bag = vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ];

        for idx in (1..self.bag.len()).rev() {
            let swap_idx = (self.rng.next_u32() as usize) % (idx + 1);
            self.bag.swap(idx, swap_idx);
        }
    }

    fn add_score(&mut self, points: i32) {
        self.score += points;
        self.best_score = self.best_score.max(self.score);
    }

    fn persist_progress(&self) {
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            score: self.score,
            lines: self.lines as i32,
            level: self.level as i32,
            board: self.board_to_value(),
            current_kind: self.current.kind.name().to_string(),
            current_rotation: self.current.rotation as i32,
            current_row: self.current.row,
            current_col: self.current.col,
            next_kind: self.next.name().to_string(),
            is_game_over: self.is_game_over,
        });
    }

    fn submit_score(&self) {
        if self.score > 0 {
            self.svc
                .submit_score_task(self.user_id, self.score, self.is_game_over);
        }
    }

    fn board_to_value(&self) -> Value {
        serde_json::to_value(self.board).unwrap_or_default()
    }
}

fn spawn_piece(kind: PieceKind) -> ActivePiece {
    ActivePiece {
        kind,
        rotation: 0,
        row: 0,
        col: 3,
    }
}

fn line_clear_score(cleared: u32, level: u32) -> i32 {
    let base = match cleared {
        1 => 100,
        2 => 300,
        3 => 500,
        4 => 800,
        _ => 0,
    };
    base * level as i32
}

pub fn piece_cells(piece: ActivePiece) -> [(i32, i32); 4] {
    let coords = piece_offsets(piece.kind, piece.rotation);
    coords.map(|(dr, dc)| (piece.row + dr, piece.col + dc))
}

fn piece_offsets(kind: PieceKind, rotation: usize) -> [(i32, i32); 4] {
    match kind {
        PieceKind::I => match rotation % 4 {
            0 | 2 => [(0, 0), (0, 1), (0, 2), (0, 3)],
            _ => [(0, 1), (1, 1), (2, 1), (3, 1)],
        },
        PieceKind::O => [(0, 1), (0, 2), (1, 1), (1, 2)],
        PieceKind::T => match rotation % 4 {
            0 => [(0, 1), (1, 0), (1, 1), (1, 2)],
            1 => [(0, 1), (1, 1), (1, 2), (2, 1)],
            2 => [(1, 0), (1, 1), (1, 2), (2, 1)],
            _ => [(0, 1), (1, 0), (1, 1), (2, 1)],
        },
        PieceKind::S => match rotation % 4 {
            0 | 2 => [(0, 1), (0, 2), (1, 0), (1, 1)],
            _ => [(0, 1), (1, 1), (1, 2), (2, 2)],
        },
        PieceKind::Z => match rotation % 4 {
            0 | 2 => [(0, 0), (0, 1), (1, 1), (1, 2)],
            _ => [(0, 2), (1, 1), (1, 2), (2, 1)],
        },
        PieceKind::J => match rotation % 4 {
            0 => [(0, 0), (1, 0), (1, 1), (1, 2)],
            1 => [(0, 1), (0, 2), (1, 1), (2, 1)],
            2 => [(1, 0), (1, 1), (1, 2), (2, 2)],
            _ => [(0, 1), (1, 1), (2, 0), (2, 1)],
        },
        PieceKind::L => match rotation % 4 {
            0 => [(0, 2), (1, 0), (1, 1), (1, 2)],
            1 => [(0, 1), (1, 1), (2, 1), (2, 2)],
            2 => [(1, 0), (1, 1), (1, 2), (2, 0)],
            _ => [(0, 0), (0, 1), (1, 1), (2, 1)],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_changes_t_piece_shape() {
        let piece = ActivePiece {
            kind: PieceKind::T,
            rotation: 0,
            row: 0,
            col: 0,
        };
        let rotated = ActivePiece {
            rotation: 1,
            ..piece
        };

        assert_ne!(piece_cells(piece), piece_cells(rotated));
    }

    #[test]
    fn line_clear_score_scales_with_level() {
        assert_eq!(line_clear_score(1, 1), 100);
        assert_eq!(line_clear_score(4, 3), 2400);
    }
}
