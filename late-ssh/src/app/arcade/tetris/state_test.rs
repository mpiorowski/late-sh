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

fn test_state() -> State {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    State::new(Uuid::nil(), LaterisService::new(db), 0)
}

#[tokio::test]
async fn first_hold_parks_the_piece_and_pulls_the_next_one_in() {
    let mut state = test_state();
    let parked = state.current.kind;
    let incoming = state.next;

    assert!(state.hold_piece());

    assert_eq!(state.hold, Some(parked));
    assert_eq!(state.current.kind, incoming);
    // The queue refilled, so holding never costs a look-ahead.
    assert_ne!(state.next, incoming);
}

#[tokio::test]
async fn second_hold_swaps_back_with_the_parked_piece() {
    let mut state = test_state();
    let first = state.current.kind;
    assert!(state.hold_piece());
    let second = state.current.kind;

    // Same piece: hold is spent until something locks.
    assert!(!state.hold_piece());
    assert_eq!(state.current.kind, second);

    state.hold_used = false;
    assert!(state.hold_piece());
    assert_eq!(state.current.kind, first);
    assert_eq!(state.hold, Some(second));
}

/// Without the one-hold-per-piece rule, hold/hold/hold swaps forever and the
/// board never advances.
#[tokio::test]
async fn hold_is_refused_until_a_piece_locks() {
    let mut state = test_state();

    assert!(state.hold_piece());
    assert!(state.hold_used);
    assert!(!state.hold_piece());

    while state.step_down(true) {}

    assert!(!state.hold_used, "locking a piece hands the hold back");
    assert!(state.hold_piece());
}

#[tokio::test]
async fn a_held_piece_respawns_at_the_top_in_its_default_rotation() {
    let mut state = test_state();
    state.current.rotation = 3;
    state.current.row = 12;
    state.current.col = 7;

    assert!(state.hold_piece());

    let fresh = spawn_piece(state.current.kind);
    assert_eq!(state.current.rotation, fresh.rotation);
    assert_eq!(state.current.row, fresh.row);
    assert_eq!(state.current.col, fresh.col);
}

#[tokio::test]
async fn ghost_sits_on_the_stack_below_the_piece() {
    let mut state = test_state();
    // A floor with one filled row; everything else empty.
    state.board[BOARD_HEIGHT - 1] = [Some(PieceKind::O); BOARD_WIDTH];
    state.current = ActivePiece {
        kind: PieceKind::I,
        rotation: 0,
        row: 0,
        col: 3,
    };

    let ghost = state.ghost_piece();
    let cells = state.view_cells();

    let ghost_cells = piece_cells(ghost);
    let piece_cells_now = piece_cells(state.current);
    // The ghost rests right on the floor row, never inside it.
    assert!(
        ghost_cells
            .iter()
            .all(|(r, _)| *r == BOARD_HEIGHT as i32 - 2)
    );
    for (row, line) in cells.iter().enumerate() {
        for (col, cell) in line.iter().enumerate() {
            let at = (row as i32, col as i32);
            let expected = if row == BOARD_HEIGHT - 1 {
                Cell::Block(PieceKind::O)
            } else if piece_cells_now.contains(&at) {
                Cell::Block(PieceKind::I)
            } else if ghost_cells.contains(&at) {
                Cell::Ghost(PieceKind::I)
            } else {
                Cell::Empty
            };
            assert_eq!(*cell, expected, "cell {at:?}");
        }
    }
}

#[tokio::test]
async fn resting_piece_hides_its_ghost() {
    let mut state = test_state();
    state.current = ActivePiece {
        kind: PieceKind::O,
        rotation: 0,
        row: 0,
        col: 4,
    };
    state.current = state.ghost_piece();

    let ghosts = state
        .view_cells()
        .iter()
        .flatten()
        .filter(|cell| matches!(cell, Cell::Ghost(_)))
        .count();
    assert_eq!(ghosts, 0);
}
