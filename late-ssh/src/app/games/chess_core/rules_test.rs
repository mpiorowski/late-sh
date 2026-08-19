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
fn chess960_shuffles_the_back_rank_within_the_rules() {
    for _ in 0..200 {
        let start = random_chess960_board();
        let text = fen(&start);
        let mut fields = text.split(' ');
        let ranks: Vec<&str> = fields.next().expect("placement").split('/').collect();
        let white: String = ranks[7].to_string();
        assert_eq!(white.len(), 8, "{text}");
        assert_eq!(
            ranks[0],
            white.to_ascii_lowercase(),
            "black mirrors: {text}"
        );
        assert_eq!((ranks[1], ranks[6]), ("pppppppp", "PPPPPPPP"), "{text}");

        let files = |piece: char| -> Vec<usize> {
            white
                .char_indices()
                .filter(|&(_, c)| c == piece)
                .map(|(index, _)| index)
                .collect()
        };
        let (bishops, rooks, king) = (files('B'), files('R'), files('K'));
        assert_eq!(
            (
                bishops.len(),
                rooks.len(),
                king.len(),
                files('Q').len(),
                files('N').len()
            ),
            (2, 2, 1, 1, 2),
            "one of each piece: {text}"
        );
        assert_ne!(
            bishops[0] % 2,
            bishops[1] % 2,
            "bishops on opposite colours: {text}"
        );
        assert!(
            rooks[0] < king[0] && king[0] < rooks[1],
            "king between its rooks: {text}"
        );

        // Castling rights name the rooks' own files, short (the rook right of
        // the king) first, so both castles survive the round trip.
        fields.next().expect("side to move");
        let file = |index: usize| (b'a' + index as u8) as char;
        assert_eq!(
            fields.next().expect("castling rights"),
            format!(
                "{}{}{}{}",
                file(rooks[1]).to_ascii_uppercase(),
                file(rooks[0]).to_ascii_uppercase(),
                file(rooks[1]),
                file(rooks[0])
            ),
            "{text}"
        );
        assert_eq!(fen(&board(&text)), text, "position reads back: {text}");
    }
}

#[test]
fn chess960_castles_by_taking_the_rook_only() {
    // King on b1 with both rooks home: c1 is empty, so it is both an ordinary
    // king step and where the long castle lands the king.
    let board = board("4k3/pppppppp/8/8/8/8/PPPPPPPP/RK5R w HA - 0 1");
    // Taking your own rook castles, either side.
    assert_eq!(legal_move_for(&board, 1, 0), Some(uci("b1a1")));
    assert_eq!(legal_move_for(&board, 1, 7), Some(uci("b1h1")));
    assert_eq!(san_label(&board, uci("b1a1")), "O-O-O");
    assert_eq!(san_label(&board, uci("b1h1")), "O-O");
    // b1c1 stays the one-step king move it looks like; the castle it would be
    // mistaken for is reached by taking the rook instead.
    assert_eq!(legal_move_for(&board, 1, 2), Some(uci("b1c1")));
    // And no landing square is advertised that cannot be played.
    let moves = legal_moves(&board);
    assert!(!moves.contains(&ChessMoveSpec { from: 1, to: 6 }));
    for spec in &moves {
        assert!(
            legal_move_for(&board, spec.from, spec.to).is_some(),
            "advertised {spec:?} resolves to nothing"
        );
    }
}

#[test]
fn persisted_fen_names_the_rook_files() {
    // Shredder notation: `HAha`, not `KQkq`, so a chess960 position can say
    // where its rooks are.
    let start = Board::default();
    assert_eq!(
        fen(&start),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1"
    );
    // The `KQkq` spelling still loads, so matches stored before the switch
    // keep playing.
    assert_eq!(
        fen(&board(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        )),
        fen(&start)
    );
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
