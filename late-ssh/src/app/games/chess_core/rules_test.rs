use cozy_chess::{Board, Move};

use super::rules::*;
use super::types::ChessMoveSpec;

/// Both rooks and both kings home, everything between them cleared, so all
/// four castles are available.
const CASTLE_FEN: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

fn board(fen: &str) -> Board {
    fen.parse().expect("test fen")
}

fn uci(mv: &str) -> Move {
    mv.parse().expect("test move")
}

#[test]
fn king_push_and_rook_click_castle_the_same_way() {
    let board = board(CASTLE_FEN);
    // e1g1 is the lichess/chess.com gesture, e1h1 the king-captures-rook pair
    // cozy-chess generates; both must land on the same castle.
    assert_eq!(legal_move_for(&board, 4, 6), Some(uci("e1h1")));
    assert_eq!(legal_move_for(&board, 4, 7), Some(uci("e1h1")));
    assert_eq!(legal_move_for(&board, 4, 2), Some(uci("e1a1")));
    assert_eq!(legal_move_for(&board, 4, 0), Some(uci("e1a1")));
    assert_eq!(
        san_label(&board, legal_move_for(&board, 4, 6).unwrap()),
        "O-O"
    );
    assert_eq!(
        san_label(&board, legal_move_for(&board, 4, 2).unwrap()),
        "O-O-O"
    );
}

#[test]
fn legal_moves_offer_both_castle_gestures() {
    let white = legal_moves(&board(CASTLE_FEN));
    for to in [7, 6, 0, 2] {
        assert!(
            white.contains(&ChessMoveSpec { from: 4, to }),
            "white king should reach {to}"
        );
    }
    // Black castles from e8 (60) onto h8 (63) / a8 (56), landing on g8 / c8.
    let black = legal_moves(&board("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R b KQkq - 0 1"));
    for to in [63, 62, 56, 58] {
        assert!(
            black.contains(&ChessMoveSpec { from: 60, to }),
            "black king should reach {to}"
        );
    }
}

#[test]
fn king_push_without_the_castling_right_is_not_a_move() {
    // White kept only the short right, so the queenside push stays illegal
    // and never shows up as a target.
    let board = board("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w Kkq - 0 1");
    assert_eq!(legal_move_for(&board, 4, 6), Some(uci("e1h1")));
    assert_eq!(legal_move_for(&board, 4, 2), None);
    assert_eq!(legal_move_for(&board, 4, 0), None);
    assert!(!legal_moves(&board).contains(&ChessMoveSpec { from: 4, to: 2 }));
}

#[test]
fn king_push_through_a_piece_is_not_a_move() {
    // Bishop on f1 blocks the short castle: the right is still in the FEN, so
    // the gesture must fail on the generated moves, not on the rights.
    let board = board("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3KB1R w KQkq - 0 1");
    assert_eq!(legal_move_for(&board, 4, 6), None);
    assert_eq!(legal_move_for(&board, 4, 7), None);
    assert!(!legal_moves(&board).contains(&ChessMoveSpec { from: 4, to: 6 }));
    assert_eq!(legal_move_for(&board, 4, 2), Some(uci("e1a1")));
}
