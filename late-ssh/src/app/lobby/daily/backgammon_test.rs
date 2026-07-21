use super::*;

fn fresh(roll: [u8; 2]) -> DailyBackgammonState {
    DailyBackgammonState {
        version: STATE_VERSION,
        revision: 0,
        white: Uuid::from_u128(1),
        red: Uuid::from_u128(2),
        turns: Vec::new(),
        next_roll: Some(roll),
    }
}

/// An empty board to place checkers on by hand.
fn bare() -> Board {
    Board {
        points: [0; POINTS],
        bar: [0; 2],
        off: [0; 2],
    }
}

#[test]
fn opening_position_and_pips() {
    let board = Board::start();
    let white: i8 = board.points.iter().filter(|n| **n > 0).sum();
    let red: i8 = board.points.iter().filter(|n| **n < 0).sum();
    assert_eq!(white, 15);
    assert_eq!(red, -15);
    assert_eq!(board.pip_count(Color::White), 167);
    assert_eq!(board.pip_count(Color::Red), 167);
    assert!(!board.all_home(Color::White));
}

#[test]
fn opening_roll_plays_both_dice() {
    let state = fresh([3, 1]);
    assert_eq!(state.turn(), Color::White);
    let legal = state.legal_turns();
    assert!(!legal.is_empty());
    assert!(legal.iter().all(|turn| turn.len() == 2));
    // The classic 31 play: 8/5 6/5 (indices 7->4, 5->4).
    assert!(
        legal
            .iter()
            .any(|turn| { turn.contains(&(7, 4)) && turn.contains(&(5, 4)) })
    );
}

#[test]
fn doubles_move_four_times() {
    let state = fresh([2, 2]);
    let legal = state.legal_turns();
    assert!(!legal.is_empty());
    assert!(legal.iter().all(|turn| turn.len() == 4));
}

#[test]
fn bar_checkers_must_enter_first() {
    let mut board = bare();
    board.bar[Color::White.idx()] = 1;
    board.points[5] = 3; // white checkers that must wait
    board.points[18] = -2; // red holds white's 6-entry (die 6)
    let legal = legal_turns(&board, Color::White, [6, 3]);
    // Die 6 cannot enter (point held); die 3 enters at index 21, then
    // the 6 plays on. Every turn starts from the bar.
    assert!(!legal.is_empty());
    assert!(legal.iter().all(|turn| turn[0] == (BAR, 21)));

    // Both entries blocked: no play at all.
    board.points[21] = -2;
    assert!(legal_turns(&board, Color::White, [6, 3]).is_empty());
}

#[test]
fn only_one_die_playable_forces_the_higher() {
    // White on 7 and 12. The 3 is dead everywhere (7->4 and 12->9 are
    // held; bearing off from 2 after 7->2 is barred while 12 sits
    // outside home), and after a 5 the leftover 3 stays dead — so every
    // legal turn is exactly one hop, and it must use the 5.
    let mut board = bare();
    board.points[7] = 1;
    board.points[12] = 1;
    board.points[4] = -2;
    board.points[9] = -2;
    let legal = legal_turns(&board, Color::White, [5, 3]);
    assert!(!legal.is_empty());
    assert!(legal.iter().all(|turn| turn.len() == 1));
    // And every single hop uses the 5.
    assert!(legal.iter().all(|turn| matches!(turn[0], (7, 2) | (12, 7))));
}

#[test]
fn hits_send_the_blot_to_the_bar() {
    let mut board = bare();
    board.points[7] = 1; // white
    board.points[4] = -1; // a red blot
    let hit = apply_hop(&mut board, Color::White, (7, 4));
    assert!(hit);
    assert_eq!(board.points[4], 1); // white owns the point now
    assert_eq!(board.bar[Color::Red.idx()], 1);
}

#[test]
fn bearing_off_needs_everyone_home_and_wastes_big_dice() {
    let mut board = bare();
    board.points[3] = 2; // white on the 4-point
    board.points[1] = 1; // and the 2-point
    assert!(board.all_home(Color::White));
    // Exact die bears off; a 6 bears off only from the farthest point.
    assert!(can_bear_off(&board, Color::White, 3, 4));
    assert!(can_bear_off(&board, Color::White, 3, 6));
    assert!(!can_bear_off(&board, Color::White, 1, 6)); // 4-point occupied
    assert!(can_bear_off(&board, Color::White, 1, 2));
    // With a checker outside home nothing bears off.
    board.points[10] = 1;
    assert!(!board.all_home(Color::White));
    assert!(
        hops_for_die(&board, Color::White, 4)
            .iter()
            .all(|&(_, to)| to != OFF)
    );
}

#[test]
fn bearing_off_the_last_checkers_wins() {
    // Two white checkers left on the 1-point: a double-1 turn is two
    // bear-offs and only two (the extra dice have nothing to move).
    let mut board = bare();
    board.points[0] = 2;
    board.off[Color::White.idx()] = 13;
    board.points[23] = -15;
    let legal = legal_turns(&board, Color::White, [1, 1]);
    assert!(!legal.is_empty());
    assert!(legal.iter().all(|turn| turn == &vec![(0, OFF), (0, OFF)]));
    let mut work = board;
    apply_hop(&mut work, Color::White, (0, OFF));
    apply_hop(&mut work, Color::White, (0, OFF));
    assert_eq!(work.off[Color::White.idx()], 15);
}

#[test]
fn stall_cap_draws_the_match() {
    let mut state = fresh([1, 2]);
    for _ in 0..STALL_PASSES {
        state.turns.push(Turn {
            roll: [1, 2],
            hops: Vec::new(),
        });
    }
    assert_eq!(state.status(), BackgammonStatus::Draw);
    assert!(state.is_finished());
}

#[test]
fn apply_turn_validates_and_records() {
    let mut state = fresh([3, 1]);
    assert!(state.apply_turn(&[(23, 20)]).is_err()); // one die short
    let outcome = state.apply_turn(&[(7, 4), (5, 4)]).unwrap();
    assert_eq!(outcome.color, Color::White);
    assert_eq!(outcome.hits, vec![false, false]);
    assert!(!outcome.finished);
    assert_eq!(outcome.label(&[(7, 4), (5, 4)]), "31: 8/5 6/5");
    assert_eq!(state.turn(), Color::Red);
    assert_eq!(state.next_roll, None);
    assert_eq!(state.move_count(), 1);
    // The server's follow-up roll restores a playable next_roll.
    state.roll_next();
    let roll = state.next_roll.unwrap();
    assert!((1..=6).contains(&roll[0]) && (1..=6).contains(&roll[1]));
    assert!(!state.legal_turns().is_empty());
}

#[test]
fn state_round_trips_through_json() {
    let mut state = fresh([3, 1]);
    state.apply_turn(&[(7, 4), (5, 4)]).unwrap();
    let value = serde_json::to_value(&state).unwrap();
    let parsed = DailyBackgammonState::parse(&value).unwrap();
    assert_eq!(parsed.turns, state.turns);
    assert_eq!(parsed.next_roll, None);
    assert_eq!(parsed.color_of(Uuid::from_u128(1)), Some(Color::White));
    assert_eq!(parsed.color_of(Uuid::from_u128(9)), None);
    assert_eq!(parsed.turn(), Color::Red);
    assert_eq!(parsed.status(), BackgammonStatus::Ongoing);
    assert_eq!(parsed.board(), state.board());
}

#[test]
fn red_notation_counts_from_its_own_side() {
    assert_eq!(point_name(Color::White, 23), "24");
    assert_eq!(point_name(Color::White, 0), "1");
    assert_eq!(point_name(Color::Red, 23), "1");
    assert_eq!(point_name(Color::Red, 0), "24");
    assert_eq!(point_name(Color::White, BAR), "bar");
    assert_eq!(point_name(Color::Red, OFF), "off");
}

#[test]
fn slots_map_both_seats() {
    // White seat: bottom-right corner is white's 1-point (index 0), top
    // right its 24-point (index 23); the mirrored seat flips them.
    assert_eq!(
        slot_target(SLOT_COLS + 12, Color::White),
        Some(BgTarget::Point(0))
    );
    assert_eq!(slot_target(12, Color::White), Some(BgTarget::Point(23)));
    assert_eq!(
        slot_target(SLOT_COLS + 12, Color::Red),
        Some(BgTarget::Point(23))
    );
    assert_eq!(slot_target(12, Color::Red), Some(BgTarget::Point(0)));
    // Bottom-left is the 12-point (index 11) from white's seat.
    assert_eq!(
        slot_target(SLOT_COLS, Color::White),
        Some(BgTarget::Point(11))
    );
    assert_eq!(slot_target(BAR_COL, Color::White), Some(BgTarget::Bar));
    assert_eq!(
        slot_target(SLOT_COLS + OFF_COL, Color::Red),
        Some(BgTarget::Off)
    );
    assert_eq!(slot_target(SLOT_ROWS * SLOT_COLS, Color::White), None);
}
