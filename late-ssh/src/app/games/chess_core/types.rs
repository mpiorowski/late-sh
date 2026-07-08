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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChessPiece {
    pub color: ChessColor,
    pub kind: ChessPieceKind,
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

pub fn piece_glyph(kind: ChessPieceKind) -> char {
    match kind {
        ChessPieceKind::Pawn => 'P',
        ChessPieceKind::Knight => 'N',
        ChessPieceKind::Bishop => 'B',
        ChessPieceKind::Rook => 'R',
        ChessPieceKind::Queen => 'Q',
        ChessPieceKind::King => 'K',
    }
}
