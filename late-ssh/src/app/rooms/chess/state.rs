use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{ChessService, ChessSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChessColor {
    White,
    Black,
}

impl ChessColor {
    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    pub fn seat_index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Black => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChessPieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChessPhase {
    Waiting,
    Ready,
    Active,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChessGameResult {
    Checkmate { winner: ChessColor },
    Timeout { winner: ChessColor },
    Resignation { winner: ChessColor },
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChessMoveSpec {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChessMoveRecord {
    pub from: usize,
    pub to: usize,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChessPieceRenderMode {
    /// Hand-drawn ASCII silhouettes, universal fallback.
    Ascii,
    /// Full-resolution PNG via Kitty/iTerm2/Sixel terminal-image protocols.
    Graphics,
}

pub struct State {
    user_id: Uuid,
    cursor: usize,
    selected: Option<usize>,
    snapshot: ChessSnapshot,
    svc: ChessService,
    snapshot_rx: watch::Receiver<ChessSnapshot>,
    piece_render_mode: ChessPieceRenderMode,
}

impl State {
    pub fn new(svc: ChessService, user_id: Uuid) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        Self {
            user_id,
            cursor: 12,
            selected: None,
            snapshot,
            svc,
            snapshot_rx,
            piece_render_mode: ChessPieceRenderMode::Graphics,
        }
    }

    pub fn piece_render_mode(&self) -> ChessPieceRenderMode {
        self.piece_render_mode
    }

    pub fn graphics_enabled(&self) -> bool {
        self.piece_render_mode == ChessPieceRenderMode::Graphics
    }

    pub fn toggle_piece_graphics(&mut self) {
        self.piece_render_mode = if self.graphics_enabled() {
            ChessPieceRenderMode::Ascii
        } else {
            ChessPieceRenderMode::Graphics
        };
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.drop_stale_selection();
        }
    }

    pub fn snapshot(&self) -> &ChessSnapshot {
        &self.snapshot
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn legal_targets(&self) -> Vec<usize> {
        let Some(selected) = self.selected else {
            return Vec::new();
        };
        self.snapshot
            .legal_moves
            .iter()
            .filter_map(|mv| (mv.from == selected).then_some(mv.to))
            .collect()
    }

    pub fn seat_index(&self) -> Option<usize> {
        self.snapshot
            .seats
            .iter()
            .position(|seat| *seat == Some(self.user_id))
    }

    pub fn user_color(&self) -> Option<ChessColor> {
        match self.seat_index()? {
            0 => Some(ChessColor::White),
            1 => Some(ChessColor::Black),
            _ => None,
        }
    }

    pub fn is_self(&self, user_id: Uuid) -> bool {
        self.user_id == user_id
    }

    pub fn sit(&self) {
        self.svc.sit_task(self.user_id);
    }

    pub fn leave_seat(&self) {
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn resign(&self) {
        self.svc.resign_task(self.user_id);
    }

    pub fn start_game(&self) {
        self.svc.start_game_task(self.user_id);
    }

    pub fn touch_activity(&self) {
        if self.seat_index().is_some() {
            self.svc.touch_activity_task(self.user_id);
        }
    }

    pub fn select_or_move(&mut self) {
        let Some(color) = self.user_color() else {
            self.sit();
            return;
        };
        if self.snapshot.phase != ChessPhase::Active {
            self.start_game();
            return;
        }
        if self.snapshot.turn != color {
            return;
        }

        if let Some(from) = self.selected {
            if from == self.cursor {
                self.selected = None;
                return;
            }
            self.svc.move_task(self.user_id, from, self.cursor);
            self.selected = None;
            return;
        }

        let Some(piece) = self
            .snapshot
            .pieces
            .get(self.cursor)
            .and_then(|piece| *piece)
        else {
            return;
        };
        if piece.color == color
            && self
                .snapshot
                .legal_moves
                .iter()
                .any(|mv| mv.from == self.cursor)
        {
            self.selected = Some(self.cursor);
        }
    }

    pub fn click_square(&mut self, index: usize) -> bool {
        if index >= 64 {
            return false;
        }
        self.cursor = index;
        if self.user_color().is_none() {
            return true;
        }
        self.select_or_move();
        true
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let (dx, dy) = match self.orienting_color() {
            ChessColor::White => (dx, dy),
            ChessColor::Black => (-dx, -dy),
        };
        let row = self.cursor / 8;
        let col = self.cursor % 8;
        let next_row = (row as isize + dy).clamp(0, 7) as usize;
        let next_col = (col as isize + dx).clamp(0, 7) as usize;
        self.cursor = next_row * 8 + next_col;
    }

    pub fn orienting_color(&self) -> ChessColor {
        self.user_color().unwrap_or(ChessColor::White)
    }

    fn drop_stale_selection(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        if self.snapshot.phase != ChessPhase::Active
            || self
                .snapshot
                .legal_moves
                .iter()
                .all(|mv| mv.from != selected)
        {
            self.selected = None;
        }
    }
}
