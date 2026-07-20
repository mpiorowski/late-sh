use super::types::{ChessColor, ChessMoveSpec};

/// Move a board cursor by one step in display coordinates, honoring board
/// orientation (Black viewers see the board flipped, so display deltas
/// invert).
pub fn move_cursor(cursor: usize, orientation: ChessColor, dx: isize, dy: isize) -> usize {
    let (dx, dy) = match orientation {
        ChessColor::White => (dx, dy),
        ChessColor::Black => (-dx, -dy),
    };
    let row = cursor / 8;
    let col = cursor % 8;
    let next_row = (row as isize + dy).clamp(0, 7) as usize;
    let next_col = (col as isize + dx).clamp(0, 7) as usize;
    next_row * 8 + next_col
}

/// Squares the selected piece can legally move to.
pub fn legal_targets(legal_moves: &[ChessMoveSpec], selected: Option<usize>) -> Vec<usize> {
    let Some(selected) = selected else {
        return Vec::new();
    };
    legal_moves
        .iter()
        .filter_map(|mv| (mv.from == selected).then_some(mv.to))
        .collect()
}


