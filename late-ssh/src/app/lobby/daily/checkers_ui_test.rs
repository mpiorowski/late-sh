use super::*;

fn fresh() -> DailyCheckersState {
    DailyCheckersState {
        version: 1,
        revision: 0,
        red: Uuid::from_u128(1),
        white: Uuid::from_u128(2),
        moves: Vec::new(),
    }
}

#[test]
fn cursor_over_a_last_move_square_wears_only_the_cursor_background() {
    // On the terminal palette the selection swap is `REVERSED`, a modifier
    // bit a later `.bg()` cannot clear, so the cell treatments must stay
    // exclusive: the cursor landing on a last-move square is a plain amber
    // cursor cell, not an inverted one.
    theme::set_current_by_id("terminal");
    let mut state = fresh();
    state
        .apply_move(&[(2, 1), (3, 0)])
        .expect("legal opening slide");

    let tier = CellTier { cw: 3, ch: 1 };
    let cursor = cell_index((3, 0));
    let lines = board_lines(&state, Some(cursor), &HashSet::new(), &HashSet::new(), tier);
    let cursor_bg = theme::AMBER_DIM();
    theme::set_current_by_id("contrast");

    // lines[0] is the header; with ch=1 each board row is one line and
    // spans[0] is the row label, so cell (3, 0) is lines[4].spans[1].
    let cell = &lines[4].spans[1];
    assert_eq!(cell.style.bg, Some(cursor_bg));
    assert!(
        !cell.style.add_modifier.contains(Modifier::REVERSED),
        "last-move inversion leaked under the cursor: {:?}",
        cell.style
    );
}
