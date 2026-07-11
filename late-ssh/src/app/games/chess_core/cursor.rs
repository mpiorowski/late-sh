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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_moves_invert_for_black_orientation() {
        // e2 (index 12): one display step "up" is toward rank 3 for White
        // but toward rank 1 for Black.
        assert_eq!(move_cursor(12, ChessColor::White, 0, 1), 20);
        assert_eq!(move_cursor(12, ChessColor::Black, 0, 1), 4);
        assert_eq!(move_cursor(12, ChessColor::White, 1, 0), 13);
        assert_eq!(move_cursor(12, ChessColor::Black, 1, 0), 11);
    }

    #[test]
    fn cursor_clamps_at_board_edges() {
        assert_eq!(move_cursor(0, ChessColor::White, -1, -1), 0);
        assert_eq!(move_cursor(63, ChessColor::White, 1, 1), 63);
    }

    #[test]
    fn legal_targets_filter_by_selected_origin() {
        let moves = [
            ChessMoveSpec { from: 12, to: 20 },
            ChessMoveSpec { from: 12, to: 28 },
            ChessMoveSpec { from: 6, to: 21 },
        ];
        assert_eq!(legal_targets(&moves, Some(12)), vec![20, 28]);
        assert!(legal_targets(&moves, None).is_empty());
    }
}
