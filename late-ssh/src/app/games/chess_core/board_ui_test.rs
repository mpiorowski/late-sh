use super::*;

fn starting_pieces() -> [Option<ChessPiece>; 64] {
    use ChessPieceKind::{Bishop, King, Knight, Pawn, Queen, Rook};
    let back = [Rook, Knight, Bishop, Queen, King, Bishop, Knight, Rook];
    let mut pieces: [Option<ChessPiece>; 64] = [None; 64];
    for file in 0..8 {
        pieces[file] = Some(ChessPiece {
            color: ChessColor::White,
            kind: back[file],
        });
        pieces[8 + file] = Some(ChessPiece {
            color: ChessColor::White,
            kind: Pawn,
        });
        pieces[48 + file] = Some(ChessPiece {
            color: ChessColor::Black,
            kind: Pawn,
        });
        pieces[56 + file] = Some(ChessPiece {
            color: ChessColor::Black,
            kind: back[file],
        });
    }
    pieces
}

#[test]
fn board_lines_keep_uniform_width_across_tiers() {
    let pieces = starting_pieces();
    for tier in TIERS {
        let ctx = BoardCtx {
            orientation: ChessColor::White,
            cursor: Some(12),
            selected: Some(8),
            last: Some((52, 36)),
            check_sq: None,
        };
        let lines = board_lines(&pieces, tier, &ctx, &[36, 28], 0);
        assert_eq!(lines.len(), tier.ch * 8 + 2, "row count for cw={}", tier.cw);
        for line in &lines {
            let width: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert_eq!(width, tier.board_w(), "line width for cw={}", tier.cw);
        }
    }
}

#[test]
fn king_square_finds_each_color() {
    let pieces = starting_pieces();
    assert_eq!(king_square(&pieces, ChessColor::White), Some(4));
    assert_eq!(king_square(&pieces, ChessColor::Black), Some(60));
}
