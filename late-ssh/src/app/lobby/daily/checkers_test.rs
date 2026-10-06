use super::*;

fn fresh() -> DailyCheckersState {
    DailyCheckersState {
        version: STATE_VERSION,
        revision: 0,
        red: Uuid::from_u128(1),
        white: Uuid::from_u128(2),
        moves: Vec::new(),
    }
}

fn man(color: Color) -> Option<Piece> {
    Some(Piece { color, king: false })
}

#[test]
fn cell_labels_read_like_a_board() {
    assert_eq!(cell_label(0, 0), "a1");
    assert_eq!(cell_label(7, 7), "h8");
    assert_eq!(cell_label(2, 3), "d3");
}

#[test]
fn red_opens_with_seven_slides_and_no_captures() {
    let state = fresh();
    assert_eq!(state.turn(), Color::Red);
    assert_eq!(state.user_of(Color::Red), Uuid::from_u128(1));
    assert_eq!(state.piece_counts(), (12, 12));
    let moves = state.legal_moves(Color::Red);
    // Only the front rank (row 2) can advance into the empty row 3.
    assert_eq!(moves.len(), 7);
    assert!(moves.iter().all(|m| m.len() == 2)); // all slides, no jumps
}

#[test]
fn a_slide_moves_the_piece_and_passes_the_turn() {
    let mut state = fresh();
    let outcome = state.apply_move(&[(2, 1), (3, 0)]).unwrap();
    assert_eq!(outcome.color, Color::Red);
    assert!(outcome.captured.is_empty());
    assert!(!outcome.crowned && !outcome.finished);
    assert_eq!(state.turn(), Color::White);
    assert_eq!(state.piece_counts(), (12, 12));
    assert_eq!(state.last_move(), Some(vec![(2, 1), (3, 0)]));
    assert_eq!(outcome.label(&[(2, 1), (3, 0)]), "b3-a4");
}

#[test]
fn a_capture_is_mandatory_and_removes_the_jumped_piece() {
    // Reach a position where red's only legal reply is a jump.
    let mut state = fresh();
    state.apply_move(&[(2, 1), (3, 2)]).unwrap(); // red advances to c4
    state.apply_move(&[(5, 4), (4, 3)]).unwrap(); // white steps beside it at d5
    // Red at c4 must take d5; a simple slide is now illegal.
    assert!(state.apply_move(&[(2, 3), (3, 4)]).is_err());
    let outcome = state.apply_move(&[(3, 2), (5, 4)]).unwrap();
    assert_eq!(outcome.captured, vec![(4, 3)]);
    assert_eq!(outcome.color, Color::Red);
    assert!(!outcome.finished);
    assert_eq!(state.piece_counts(), (12, 11));
    assert_eq!(outcome.label(&[(3, 2), (5, 4)]), "c4xe6");
}

#[test]
fn a_double_jump_chains_into_one_move() {
    let mut grid = [[None; SIZE]; SIZE];
    grid[2][1] = man(Color::Red);
    grid[3][2] = man(Color::White);
    grid[5][4] = man(Color::White);
    let moves = generate_moves(&grid, Color::Red);
    assert_eq!(moves, vec![vec![(2, 1), (4, 3), (6, 5)]]);

    apply_path(&mut grid, &moves[0]);
    assert_eq!(grid[6][5], man(Color::Red)); // landed, still a man
    assert!(grid[3][2].is_none() && grid[5][4].is_none()); // both taken
    assert!(grid[2][1].is_none());
}

#[test]
fn crowning_ends_the_turn_mid_chain() {
    // A man reaching the back rank is crowned and its turn ends at once:
    // from c6 red jumps over d7 to e8 and stops, even though a king could
    // continue back over f7. So only the single-jump chain is legal.
    let mut grid = [[None; SIZE]; SIZE];
    grid[5][2] = man(Color::Red);
    grid[6][3] = man(Color::White);
    grid[6][5] = man(Color::White);
    assert_eq!(
        generate_moves(&grid, Color::Red),
        vec![vec![(5, 2), (7, 4)]]
    );

    apply_path(&mut grid, &[(5, 2), (7, 4)]);
    assert_eq!(
        grid[7][4],
        Some(Piece {
            color: Color::Red,
            king: true
        })
    );
    assert!(grid[6][3].is_none());
    assert_eq!(grid[6][5], man(Color::White)); // the tempting piece survives
}

#[test]
fn a_blocked_side_has_no_move() {
    // Lone red man at b1, boxed in: both diagonals hold white men and the
    // jump landings are occupied, so red cannot move (and would lose).
    let mut grid = [[None; SIZE]; SIZE];
    grid[0][1] = man(Color::Red);
    grid[1][0] = man(Color::White);
    grid[1][2] = man(Color::White);
    grid[2][3] = man(Color::White); // blocks the b1xd3 landing
    assert!(generate_moves(&grid, Color::Red).is_empty());
    assert!(!generate_moves(&grid, Color::White).is_empty());
}

#[test]
fn state_round_trips_through_json() {
    let mut state = DailyCheckersState {
        version: STATE_VERSION,
        revision: 0,
        red: Uuid::from_u128(7),
        white: Uuid::from_u128(8),
        moves: Vec::new(),
    };
    state.apply_move(&[(2, 1), (3, 2)]).unwrap();
    let value = serde_json::to_value(&state).unwrap();
    let parsed = DailyCheckersState::parse(&value).unwrap();
    assert_eq!(
        parsed.moves,
        vec![vec![(2 * SIZE + 1) as u8, (3 * SIZE + 2) as u8]]
    );
    assert_eq!(parsed.color_of(Uuid::from_u128(7)), Some(Color::Red));
    assert_eq!(parsed.color_of(Uuid::from_u128(9)), None);
    assert_eq!(parsed.turn(), Color::White);
    assert_eq!(parsed.status(), CheckersStatus::Ongoing);
}
