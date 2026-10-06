use super::*;
use crate::app::lobby::house::tron::settings::TronMode;

fn state_with_two_players() -> (SharedState, Uuid, Uuid) {
    state_with_two_players_and_settings(TronTableSettings::default())
}

fn state_with_two_players_and_settings(settings: TronTableSettings) -> (SharedState, Uuid, Uuid) {
    let mut state = SharedState::new(Uuid::now_v7(), settings);
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    state.sit(a);
    state.sit(b);
    (state, a, b)
}

#[test]
fn start_requires_two_riders() {
    let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
    let user = Uuid::now_v7();
    state.sit(user);
    assert!(state.start_round(user).is_none());
    assert_eq!(state.phase, TronPhase::Waiting);
}

#[test]
fn rejects_direct_reverse_turns() {
    let (mut state, user, _) = state_with_two_players();
    state.start_round(user);
    state.steer(user, Direction::Left);
    assert_eq!(state.pending_directions[0], Direction::Right);
}

#[test]
fn wall_crash_can_produce_a_winner() {
    let (mut state, user, _) = state_with_two_players();
    let tick_loop = state.start_round(user).unwrap();
    state.players[0].head = Some(Position { x: 0, y: 0 });
    state.players[0].direction = Direction::Left;
    state.pending_directions[0] = Direction::Left;
    let outcome = state.tick_generation(tick_loop.generation);
    let game_end = outcome.game_end.expect("round should end");
    assert!(game_end.win.is_some());
    assert!(game_end.played.is_empty());
    assert_eq!(state.phase, TronPhase::Finished);
    assert_eq!(state.outcome, Some(TronOutcome::Winner { seat_index: 1 }));
}

#[test]
fn head_on_collision_draws_when_no_riders_survive() {
    let (mut state, user, _) = state_with_two_players();
    let tick_loop = state.start_round(user).unwrap();
    state.board = [None; BOARD_CELLS];
    state.pickups = [None; BOARD_CELLS];
    state.players[0].head = Some(Position { x: 10, y: 10 });
    state.players[0].direction = Direction::Right;
    state.pending_directions[0] = Direction::Right;
    state.players[1].head = Some(Position { x: 12, y: 10 });
    state.players[1].direction = Direction::Left;
    state.pending_directions[1] = Direction::Left;
    state.board[Position { x: 10, y: 10 }.index()] = Some(0);
    state.board[Position { x: 12, y: 10 }.index()] = Some(1);
    let outcome = state.tick_generation(tick_loop.generation);
    let game_end = outcome.game_end.expect("round should end");
    assert!(game_end.win.is_none());
    assert!(game_end.played.is_empty());
    assert_eq!(state.outcome, Some(TronOutcome::Draw));
}

#[test]
fn gaps_mode_skips_every_seventh_trail_cell() {
    let (mut state, user, _) = state_with_two_players_and_settings(TronTableSettings {
        speed: Default::default(),
        mode: TronMode::Gaps,
    });
    let tick_loop = state.start_round(user).unwrap();
    for _ in 0..GAP_PERIOD {
        let outcome = state.tick_generation(tick_loop.generation);
        assert!(outcome.ticked);
    }

    let gap = Position {
        x: (BOARD_WIDTH / 4) as u8 + GAP_PERIOD as u8,
        y: (BOARD_HEIGHT / 2) as u8,
    };
    assert_eq!(state.players[0].head, Some(gap));
    assert_eq!(state.board[gap.index()], None);
}

#[test]
fn phase_charge_passes_through_one_trail_cell() {
    let (mut state, user, _) = state_with_two_players();
    let tick_loop = state.start_round(user).unwrap();
    state.board = [None; BOARD_CELLS];
    state.pickups = [None; BOARD_CELLS];
    state.players[0].head = Some(Position { x: 10, y: 10 });
    state.players[0].direction = Direction::Right;
    state.players[0].phase_charges = 1;
    state.pending_directions[0] = Direction::Right;
    state.players[1].head = Some(Position { x: 40, y: 10 });
    state.players[1].direction = Direction::Right;
    state.pending_directions[1] = Direction::Right;
    state.board[Position { x: 10, y: 10 }.index()] = Some(0);
    state.board[Position { x: 11, y: 10 }.index()] = Some(1);
    state.board[Position { x: 40, y: 10 }.index()] = Some(1);

    state.tick_generation(tick_loop.generation);

    let phased_cell = Position { x: 11, y: 10 };
    assert!(state.players[0].alive);
    assert_eq!(state.players[0].head, Some(phased_cell));
    assert_eq!(state.players[0].phase_charges, 0);
    assert_eq!(state.board[phased_cell.index()], Some(1));
}

#[test]
fn boost_moves_two_cells_and_trails_both() {
    let (mut state, user, _) = state_with_two_players();
    let tick_loop = state.start_round(user).unwrap();
    state.board = [None; BOARD_CELLS];
    state.pickups = [None; BOARD_CELLS];
    let start = Position { x: 10, y: 10 };
    state.players[0].head = Some(start);
    state.players[0].direction = Direction::Right;
    state.players[0].boost_ticks = 1;
    state.pending_directions[0] = Direction::Right;
    state.players[1].head = Some(Position { x: 40, y: 10 });
    state.players[1].direction = Direction::Right;
    state.pending_directions[1] = Direction::Right;
    state.board[start.index()] = Some(0);
    state.board[Position { x: 40, y: 10 }.index()] = Some(1);

    state.tick_generation(tick_loop.generation);

    let mid = Position { x: 11, y: 10 };
    let end = Position { x: 12, y: 10 };
    assert!(state.players[0].alive);
    // Two cells crossed in one tick, trail laid on both, charge spent.
    assert_eq!(state.players[0].head, Some(end));
    assert_eq!(state.board[mid.index()], Some(0));
    assert_eq!(state.board[end.index()], Some(0));
    assert_eq!(state.players[0].boost_ticks, 0);
}

#[test]
fn crashed_rider_leaving_does_not_clear_running_round() {
    let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
    let crashed_user = Uuid::now_v7();
    let alive_a = Uuid::now_v7();
    let alive_b = Uuid::now_v7();
    state.sit(crashed_user);
    state.sit(alive_a);
    state.sit(alive_b);
    state.start_round(crashed_user);
    state.players[0].alive = false;
    state.players[0].crashed = true;

    let game_end = state.leave(crashed_user);

    assert!(game_end.is_none());
    assert_eq!(state.phase, TronPhase::Running);
    assert_eq!(state.seats[0], None);
    assert!(state.players[1].alive);
    assert!(state.players[2].alive);
    assert!(state.board.iter().any(Option::is_some));
}

#[test]
fn inactive_crashed_rider_does_not_clear_running_round() {
    let mut state = SharedState::new(Uuid::now_v7(), TronTableSettings::default());
    let crashed_user = Uuid::now_v7();
    let alive_a = Uuid::now_v7();
    let alive_b = Uuid::now_v7();
    state.sit(crashed_user);
    state.sit(alive_a);
    state.sit(alive_b);
    state.start_round(crashed_user);
    state.players[0].alive = false;
    state.players[0].crashed = true;
    state.last_activity[0] = Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);
    let generation = state.activity_generation[0];

    let outcome = state.kick_inactive_user(crashed_user, generation);

    assert!(outcome.changed);
    assert!(outcome.game_end.is_none());
    assert_eq!(state.phase, TronPhase::Running);
    assert_eq!(state.seats[0], None);
    assert!(state.players[1].alive);
    assert!(state.players[2].alive);
    assert!(state.board.iter().any(Option::is_some));
}

#[test]
fn payout_is_flat_across_multiplayer_rounds() {
    assert_eq!(tron_win_payout(1), 0);
    assert_eq!(tron_win_payout(2), TRON_WIN_CHIPS);
    assert_eq!(tron_win_payout(3), TRON_WIN_CHIPS);
    assert_eq!(tron_win_payout(4), TRON_WIN_CHIPS);
}

#[test]
fn played_event_requires_minimum_round_ticks() {
    let (mut state, user_a, user_b) = state_with_two_players();
    state.start_round(user_a);
    state.round_tick_count = TRON_PLAYED_MIN_TICKS;
    state.players[0].alive = false;
    state.players[0].crashed = true;

    let game_end = state.finish_if_needed().expect("round should end");

    assert_eq!(game_end.played, vec![user_a, user_b]);
}
