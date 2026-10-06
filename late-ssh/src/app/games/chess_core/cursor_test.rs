use super::types::{ChessColor, ChessMoveSpec};
use crate::app::games::chess_core::cursor::*;

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
