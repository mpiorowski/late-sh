use super::*;

fn fresh() -> DailyConnect4State {
    DailyConnect4State {
        version: STATE_VERSION,
        revision: 0,
        red: Uuid::from_u128(1),
        yellow: Uuid::from_u128(2),
        drops: Vec::new(),
    }
}

fn play(state: &mut DailyConnect4State, columns: &[usize]) -> DropOutcome {
    let mut last = None;
    for &column in columns {
        let outcome = state.apply_drop(column).unwrap();
        last = Some(outcome);
    }
    last.unwrap()
}

#[test]
fn turns_alternate_starting_with_red() {
    let mut state = fresh();
    assert_eq!(state.turn(), Disc::Red);
    assert_eq!(state.user_of(state.turn()), Uuid::from_u128(1));
    let outcome = state.apply_drop(3).unwrap();
    assert_eq!(outcome.disc, Disc::Red);
    assert_eq!((outcome.row, state.last_drop()), (0, Some((0, 3))));
    assert_eq!(state.turn(), Disc::Yellow);
    assert_eq!(state.apply_drop(3).unwrap().row, 1);
}

#[test]
fn vertical_line_wins() {
    let mut state = fresh();
    let outcome = play(&mut state, &[0, 1, 0, 1, 0, 1]);
    assert!(!outcome.connected);
    let win = state.apply_drop(0).unwrap();
    assert!(win.connected);
    assert_eq!(win.disc, Disc::Red);
    assert_eq!(win.label(0), "a4, four in a row");
}

#[test]
fn horizontal_line_wins() {
    let mut state = fresh();
    let win = play(&mut state, &[0, 0, 1, 1, 2, 2, 3]);
    assert!(win.connected);
    assert_eq!(win.disc, Disc::Red);
}

#[test]
fn diagonal_line_wins() {
    let mut state = fresh();
    // Red builds (0,0) (1,1) (2,2) (3,3); yellow's replies stay inert.
    let win = play(&mut state, &[0, 1, 1, 2, 2, 3, 2, 3, 3, 6, 3]);
    assert!(win.connected);
    assert_eq!(win.disc, Disc::Red);
    assert_eq!(
        state.winning_line(),
        Some(vec![(0, 0), (1, 1), (2, 2), (3, 3)])
    );
}

#[test]
fn full_column_and_off_board_are_rejected() {
    let mut state = fresh();
    play(&mut state, &[0, 0, 0, 0, 0, 0]);
    let full = state.apply_drop(0);
    assert!(full.unwrap_err().to_string().contains("column a is full"));
    let off = state.apply_drop(COLS);
    assert!(off.unwrap_err().to_string().contains("off the board"));
}

/// A concrete drop order that fills all 42 cells without ever connecting
/// four. Column-cycling can't do this: with 7 columns the disc colors fall
/// into a checkerboard whose `\` diagonals are monochrome, so Red connects
/// on the main diagonal long before the board fills. This order was found by
/// searching for a sequence where no drop ever completes a line.
const DRAW_ORDER: [usize; CELLS] = [
    4, 5, 4, 2, 3, 1, 3, 0, 2, 3, 3, 4, 2, 2, 2, 3, 0, 3, 2, 1, 4, 5, 1, 4, 5, 6, 0, 6, 4, 5,
    5, 0, 0, 1, 0, 1, 5, 1, 6, 6, 6, 6,
];

#[test]
fn filling_every_cell_without_a_line_is_a_draw() {
    let mut state = fresh();
    for (index, column) in DRAW_ORDER.into_iter().enumerate() {
        let outcome = state.apply_drop(column).unwrap();
        assert!(!outcome.connected);
        assert_eq!(outcome.draw, index == CELLS - 1);
    }
    assert_eq!(state.move_count(), CELLS);
}

#[test]
fn state_round_trips_through_json() {
    let mut state = DailyConnect4State::new(Uuid::from_u128(7), Uuid::from_u128(8));
    state.apply_drop(3).unwrap();
    let value = serde_json::to_value(&state).unwrap();
    let parsed = DailyConnect4State::parse(&value).unwrap();
    assert_eq!(parsed.drops, vec![3]);
    assert_eq!(parsed.disc_of(state.red), Some(Disc::Red));
    assert_eq!(parsed.disc_of(Uuid::from_u128(9)), None);
}
