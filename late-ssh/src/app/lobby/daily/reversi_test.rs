use super::*;

fn fresh() -> DailyReversiState {
    DailyReversiState {
        version: STATE_VERSION,
        revision: 0,
        black: Uuid::from_u128(1),
        white: Uuid::from_u128(2),
        moves: Vec::new(),
    }
}

#[test]
fn cell_labels_read_like_a_board() {
    assert_eq!(cell_label(0, 0), "a1");
    assert_eq!(cell_label(7, 7), "h8");
    assert_eq!(cell_label(2, 3), "d3");
}

#[test]
fn black_opens_from_the_standard_four_moves() {
    let state = fresh();
    assert_eq!(state.turn(), Disc::Black);
    assert_eq!(state.user_of(Disc::Black), Uuid::from_u128(1));
    assert_eq!(state.disc_counts(), (2, 2));
    let mut moves = state.legal_moves(Disc::Black);
    moves.sort();
    // c4, d3, f5, e6 — the four symmetric diagonal openings.
    assert_eq!(moves, vec![(2, 3), (3, 2), (4, 5), (5, 4)]);
}

#[test]
fn a_move_flips_the_bracketed_disc() {
    let mut state = fresh();
    let outcome = state.apply_move(2, 3).unwrap(); // d3
    assert_eq!(outcome.disc, Disc::Black);
    assert_eq!(outcome.flipped, vec![(3, 3)]);
    assert!(!outcome.finished);
    assert_eq!(state.disc_counts(), (4, 1));
    assert_eq!(state.turn(), Disc::White);
    assert_eq!(state.last_move(), Some((2, 3)));
    assert_eq!(outcome.label(2, 3), "d3");
}

#[test]
fn preview_flips_matches_the_applied_move() {
    let state = fresh();
    assert_eq!(state.preview_flips(2, 3, Disc::Black), vec![(3, 3)]);
    assert!(state.preview_flips(0, 0, Disc::Black).is_empty());
}

#[test]
fn illegal_moves_are_rejected() {
    let mut state = fresh();
    let occupied = state.apply_move(3, 3).unwrap_err().to_string();
    assert!(occupied.contains("already taken"), "{occupied}");
    let off = state.apply_move(SIZE, 0).unwrap_err().to_string();
    assert!(off.contains("off the board"), "{off}");
    let no_flip = state.apply_move(0, 0).unwrap_err().to_string(); // a1 brackets nothing
    assert!(no_flip.contains("flips nothing"), "{no_flip}");
}

#[test]
fn a_side_with_no_bracket_must_pass() {
    // Black at a1, white at b1: black can bracket white by playing c1, but
    // white has no black disc it can bracket, so white must pass.
    let mut grid = [[None; SIZE]; SIZE];
    grid[0][0] = Some(Disc::Black);
    grid[0][1] = Some(Disc::White);
    assert!(legal_cells(&grid, Disc::White).is_empty());
    assert_eq!(legal_cells(&grid, Disc::Black), vec![(0, 2)]);
}

#[test]
fn self_play_reaches_a_finished_result() {
    let mut state = fresh();
    let mut last = None;
    while !state.is_finished() {
        let disc = state.turn();
        let moves = state.legal_moves(disc);
        assert!(
            !moves.is_empty(),
            "the player on the clock always has a move"
        );
        let (row, col) = moves[0];
        let outcome = state.apply_move(row, col).unwrap();
        assert_eq!(outcome.disc, disc);
        assert!(state.move_count() <= CELLS);
        last = Some(outcome);
    }
    let outcome = last.expect("at least one move was played");
    assert!(outcome.finished);
    let (black, white) = state.disc_counts();
    let expected = match black.cmp(&white) {
        Ordering::Greater => Some(Disc::Black),
        Ordering::Less => Some(Disc::White),
        Ordering::Equal => None,
    };
    assert_eq!(outcome.winner, expected);
    assert_eq!(outcome.draw, expected.is_none());
    assert!((4..=CELLS).contains(&(black + white)));
}

#[test]
fn state_round_trips_through_json() {
    let mut state = DailyReversiState {
        version: STATE_VERSION,
        revision: 0,
        black: Uuid::from_u128(7),
        white: Uuid::from_u128(8),
        moves: Vec::new(),
    };
    state.apply_move(2, 3).unwrap(); // d3
    let value = serde_json::to_value(&state).unwrap();
    let parsed = DailyReversiState::parse(&value).unwrap();
    assert_eq!(parsed.moves, vec![(2 * SIZE + 3) as u8]);
    assert_eq!(parsed.disc_of(Uuid::from_u128(7)), Some(Disc::Black));
    assert_eq!(parsed.disc_of(Uuid::from_u128(9)), None);
    assert_eq!(parsed.turn(), Disc::White);
}
