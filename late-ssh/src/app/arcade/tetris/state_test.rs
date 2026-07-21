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
