use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square, util::display_san_move};
use rand::Rng;

use super::types::{ChessColor, ChessMoveSpec, ChessPiece, ChessPieceKind};

/// The FEN we persist, for every variant: Shredder notation, where the
/// castling rights name the rooks' own files (`HAha` for the standard
/// opening) instead of `KQkq`.
///
/// `KQkq` cannot describe a rook that does not stand on a1/h1, so a Chess960
/// position written that way does not read back. One notation for both
/// variants means no code path has to know which one it is holding, and
/// `str::parse` accepts either, so positions stored before this still load.
pub fn fen(board: &Board) -> String {
    format!("{board:#}")
}

/// A random Chess960 starting position: the back rank shuffled under the two
/// rules that keep the game recognisable — the bishops stand on opposite
/// colours, and the king stands between its two rooks, so both castles exist.
/// Black mirrors White, as Chess960 requires.
pub fn random_chess960_board() -> Board {
    let mut rng = rand::thread_rng();
    let mut back = [' '; 8];
    // Bishops first, one on an even file and one on an odd file: that is the
    // opposite-colours rule, and doing it first means it cannot be violated.
    back[rng.gen_range(0..4) * 2] = 'B';
    back[rng.gen_range(0..4) * 2 + 1] = 'B';
    // Queen and knights drop into whatever is still free.
    for piece in ['Q', 'N', 'N'] {
        let free: Vec<usize> = (0..8).filter(|&file| back[file] == ' ').collect();
        back[free[rng.gen_range(0..free.len())]] = piece;
    }
    // The three squares left over take rook, king, rook from left to right,
    // which is what puts the king between its rooks.
    let free: Vec<usize> = (0..8).filter(|&file| back[file] == ' ').collect();
    let (long_rook, short_rook) = (free[0], free[2]);
    back[free[0]] = 'R';
    back[free[1]] = 'K';
    back[free[2]] = 'R';

    let white: String = back.iter().collect();
    let black = white.to_ascii_lowercase();
    let long = (b'a' + long_rook as u8) as char;
    let short = (b'a' + short_rook as u8) as char;
    let rights = format!(
        "{}{}{}{}",
        short.to_ascii_uppercase(),
        long.to_ascii_uppercase(),
        short,
        long
    );
    let fen = format!("{black}/pppppppp/8/8/8/8/PPPPPPPP/{white} w {rights} - 0 1");
    fen.parse()
        .expect("generated chess960 position is a legal board")
}

pub fn chess_color(color: Color) -> ChessColor {
    match color {
        Color::White => ChessColor::White,
        Color::Black => ChessColor::Black,
    }
}

pub fn chess_piece_kind(piece: Piece) -> ChessPieceKind {
    match piece {
        Piece::Pawn => ChessPieceKind::Pawn,
        Piece::Knight => ChessPieceKind::Knight,
        Piece::Bishop => ChessPieceKind::Bishop,
        Piece::Rook => ChessPieceKind::Rook,
        Piece::Queen => ChessPieceKind::Queen,
        Piece::King => ChessPieceKind::King,
    }
}

pub fn board_pieces(board: &Board) -> [Option<ChessPiece>; 64] {
    std::array::from_fn(|index| {
        let square = Square::index(index);
        let piece = board.piece_on(square)?;
        let color = board.color_on(square)?;
        Some(ChessPiece {
            color: chess_color(color),
            kind: chess_piece_kind(piece),
        })
    })
}

/// Every from/to pair the board accepts for the side to move. A castle shows
/// up twice: once the way cozy-chess encodes it (the king capturing its own
/// rook, so selecting the king and then the rook castles) and once as the king
/// pushed two squares toward the rook, the gesture lichess and chess.com use.
/// Both pairs resolve to the same move through `legal_move_for`. The second
/// pair is only offered from e1/e8; see `castle_king_landing`.
pub fn legal_moves(board: &Board) -> Vec<ChessMoveSpec> {
    let mut moves = Vec::new();
    board.generate_moves(|piece_moves| {
        for mv in piece_moves {
            moves.push(ChessMoveSpec {
                from: mv.from as usize,
                to: mv.to as usize,
            });
            if let Some(landing) = castle_king_landing(board, mv) {
                moves.push(ChessMoveSpec {
                    from: mv.from as usize,
                    to: landing as usize,
                });
            }
        }
        false
    });
    moves
}

/// The square the king lands on when `mv` is a castle: two files toward the
/// rook, which is the square lichess and chess.com click. `None` for every
/// other move.
///
/// cozy-chess speaks Chess960 notation, where a castle is the king capturing
/// its own rook, so `mv.to` is the rook's square rather than the king's
/// destination. A king move onto a square held by its own side can only be
/// that encoding.
///
/// Offered from e1/e8 only, the same guard `castle_rook_target` uses, so no
/// square is advertised that cannot be resolved back. A Chess960 king that
/// starts elsewhere castles by taking its rook and nothing else: from b1 the
/// two-square push is `b1c1`, which is also an ordinary king step, and
/// resolving that pair to a castle would play a move nobody asked for.
fn castle_king_landing(board: &Board, mv: Move) -> Option<Square> {
    let side = board.side_to_move();
    let rank = Rank::First.relative_to(side);
    if board.king(side) != mv.from || mv.from != Square::new(File::E, rank) {
        return None;
    }
    let rights = board.castle_rights(side);
    let short = rights.short.map(|file| Square::new(file, rank));
    let long = rights.long.map(|file| Square::new(file, rank));
    match mv.to {
        to if Some(to) == short => Some(Square::new(File::G, rank)),
        to if Some(to) == long => Some(Square::new(File::C, rank)),
        _ => None,
    }
}

/// The rook square a two-square king push means, so the lichess/chess.com
/// castling gesture resolves to the move cozy-chess generated. `None` when the
/// pair is not that gesture, in which case it is taken literally.
///
/// Recognized from e1/e8 only — always the case in standard chess, sometimes
/// in Chess960 — because a king standing anywhere else has ordinary one-step
/// moves onto c1/g1 that this must not swallow. Matching on the stored rights
/// (not on a fixed h/a file) keeps the mapping honest even when only one side
/// of the board still has a right, which is the common Chess960 case.
fn castle_rook_target(board: &Board, from: Square, to: Square) -> Option<Square> {
    let side = board.side_to_move();
    let rank = Rank::First.relative_to(side);
    if board.king(side) != from || from != Square::new(File::E, rank) {
        return None;
    }
    let rights = board.castle_rights(side);
    match to {
        to if to == Square::new(File::G, rank) => rights.short.map(|f| Square::new(f, rank)),
        to if to == Square::new(File::C, rank) => rights.long.map(|f| Square::new(f, rank)),
        _ => None,
    }
}

/// Resolve a from/to pair coming off the board into a legal move, promoting to
/// a queen when the pair is a promotion. Accepts either castling gesture: the
/// king onto its rook, or the king pushed two squares toward it.
pub fn legal_move_for(board: &Board, from: usize, to: usize) -> Option<Move> {
    let from_square = Square::try_index(from)?;
    let to = match castle_rook_target(board, from_square, Square::try_index(to)?) {
        Some(rook) => rook as usize,
        None => to,
    };
    let mut fallback = None;
    let mut queen = None;
    board.generate_moves(|piece_moves| {
        for mv in piece_moves {
            if mv.from as usize == from && mv.to as usize == to {
                if mv.promotion == Some(Piece::Queen) {
                    queen = Some(mv);
                    return true;
                }
                fallback.get_or_insert(mv);
            }
        }
        false
    });
    queen.or(fallback)
}

pub fn san_label(board: &Board, mv: Move) -> String {
    format!("{}", display_san_move(board, mv))
}

/// How many positions in `history` repeat the current position (the current
/// position is expected to already be part of `history`).
pub fn repetition_count(history: &[Board], current: &Board) -> usize {
    history
        .iter()
        .filter(|position| position.same_position(current))
        .count()
}
