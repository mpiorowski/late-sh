use chrono::{Duration, NaiveDate, Utc};
use late_core::db::{Db, DbConfig};
use late_core::models::{
    chips::Difficulty,
    sliding_puzzle::{DailyWin, Game},
};
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    state::{
        Direction, State, apply_blank_move, board_dimension, board_len, generate_scramble,
        solved_board,
    },
    svc::SlidingPuzzleService,
};
use crate::{
    app::activity::event::{ActivityEvent, ActivityGame, ActivityKind},
    test_helpers::new_test_db,
};

fn service() -> SlidingPuzzleService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    SlidingPuzzleService::new(db, activity)
}

fn game(
    user_id: Uuid,
    puzzle_date: NaiveDate,
    difficulty: Difficulty,
    tiles: Vec<i32>,
    moves: i32,
) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id,
        mode: "daily".to_string(),
        puzzle_date: Some(puzzle_date),
        difficulty_key: difficulty.key().to_string(),
        puzzle_seed: super::state::daily_seed(puzzle_date, difficulty) as i64,
        tiles,
        moves,
    }
}

fn personal_game(
    user_id: Uuid,
    difficulty: Difficulty,
    seed: u64,
    tiles: Vec<i32>,
    moves: i32,
) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id,
        mode: "personal".to_string(),
        puzzle_date: None,
        difficulty_key: difficulty.key().to_string(),
        puzzle_seed: seed as i64,
        tiles,
        moves,
    }
}

#[test]
fn difficulties_have_exact_board_position_counts() {
    assert_eq!(board_len(Difficulty::Easy), 9);
    assert_eq!(board_len(Difficulty::Medium), 16);
    assert_eq!(board_len(Difficulty::Hard), 25);
}

#[test]
fn generated_boards_are_permutations_unsolved_deterministic_and_reachable() {
    for difficulty in Difficulty::ALL {
        let generated = generate_scramble(*difficulty, 0x5eed);
        assert_eq!(generated, generate_scramble(*difficulty, 0x5eed));
        assert_ne!(generated.tiles, solved_board(*difficulty));

        let mut values = generated.tiles.clone();
        values.sort_unstable();
        assert_eq!(
            values,
            (0..board_len(*difficulty) as u8).collect::<Vec<_>>()
        );
        assert_eq!(generated.tiles.iter().filter(|&&tile| tile == 0).count(), 1);

        let mut replay = solved_board(*difficulty);
        for &direction in &generated.blank_moves {
            assert!(apply_blank_move(
                &mut replay,
                board_dimension(*difficulty),
                direction
            ));
        }
        assert_eq!(replay, generated.tiles);

        for &direction in generated.blank_moves.iter().rev() {
            assert!(apply_blank_move(
                &mut replay,
                board_dimension(*difficulty),
                direction.inverse()
            ));
        }
        assert_eq!(replay, solved_board(*difficulty));
    }
}

/// A daily board must not hand out a head start. Over a fixed year of UTC
/// dates no tile may already sit in its own solved cell on more than 30% of
/// the days; a well-mixed scramble lands near the 1-in-`board_len` chance.
#[test]
fn daily_scrambles_do_not_park_a_tile_in_its_solved_cell() {
    const DAYS: i64 = 365;
    let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

    for difficulty in Difficulty::ALL {
        let solved = solved_board(*difficulty);
        let mut home_days = vec![0usize; board_len(*difficulty)];
        for day in 0..DAYS {
            let date = start + Duration::days(day);
            let tiles =
                generate_scramble(*difficulty, super::state::daily_seed(date, *difficulty)).tiles;
            for (cell, tile) in tiles.iter().enumerate() {
                if *tile == solved[cell] {
                    home_days[cell] += 1;
                }
            }
        }

        let (cell, days_home) = home_days
            .into_iter()
            .enumerate()
            .max_by_key(|(_, days_home)| *days_home)
            .expect("board has cells");
        assert!(
            days_home as i64 * 10 <= DAYS * 3,
            "{} daily boards start with tile {} already in cell {cell} on {days_home}/{DAYS} days",
            difficulty.key(),
            solved[cell],
        );
    }
}

#[test]
fn blank_moves_on_a_board_that_does_not_match_its_dimension_are_refused() {
    let mut tiles = vec![0, 1];

    assert!(!apply_blank_move(&mut tiles, 3, Direction::Down));
    assert_eq!(tiles, vec![0, 1]);
}

#[test]
fn legal_moves_advance_blank_and_count_but_illegal_moves_do_not() {
    let user_id = Uuid::now_v7();
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let mut state = State::new_for_date(user_id, service(), date, Vec::new());
    state.set_board_for_test(Difficulty::Easy, vec![0, 1, 2, 3, 4, 5, 6, 7, 8], 0);

    assert!(!state.move_blank(Direction::Up));
    assert!(!state.move_blank(Direction::Left));
    assert_eq!(state.moves(), 0);
    assert!(state.move_blank(Direction::Right));
    assert_eq!(state.board(), &[1, 0, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(state.moves(), 1);
}

#[test]
fn each_adjacent_clicked_tile_moves_into_the_blank() {
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let cases = [
        (1, vec![1, 0, 3, 4, 2, 5, 6, 7, 8]),
        (7, vec![1, 2, 3, 4, 7, 5, 6, 0, 8]),
        (3, vec![1, 2, 3, 0, 4, 5, 6, 7, 8]),
        (5, vec![1, 2, 3, 4, 5, 0, 6, 7, 8]),
    ];

    for (index, expected) in cases {
        let mut state = State::new_for_date(Uuid::now_v7(), service(), date, Vec::new());
        state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 0, 5, 6, 7, 8], 0);

        assert!(state.move_tile(index));
        assert_eq!(state.board(), expected);
        assert_eq!(state.moves(), 1);
    }
}

#[test]
fn nonadjacent_blank_and_out_of_bounds_clicks_are_noops() {
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let board = vec![1, 2, 3, 4, 0, 5, 6, 7, 8];

    for index in [0, 4, 9] {
        let mut state = State::new_for_date(Uuid::now_v7(), service(), date, Vec::new());
        state.set_board_for_test(Difficulty::Easy, board.clone(), 0);

        assert!(!state.move_tile(index));
        assert_eq!(state.board(), board);
        assert_eq!(state.moves(), 0);
    }
}

#[tokio::test]
async fn daily_and_personal_modes_restore_independent_progress() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-independent-modes").await;
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new_for_date(user.id, service.clone(), date, Vec::new());

    state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 0, 5, 6, 7, 8], 2);
    assert!(state.move_blank(Direction::Right));
    let daily_board = state.board().to_vec();

    state.show_personal();
    assert_eq!(state.mode, super::state::Mode::Personal);
    assert_eq!(state.reward_chips(), None);
    state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 0, 5, 6, 7, 8], 7);
    assert!(state.move_blank(Direction::Down));
    let personal_board = state.board().to_vec();

    state.show_daily();
    assert_eq!(state.board(), daily_board);
    assert_eq!(state.moves(), 3);
    assert_eq!(state.reward_chips(), Some(Difficulty::Easy.chips()));

    state.show_personal();
    assert_eq!(state.board(), personal_board);
    assert_eq!(state.moves(), 8);
    service.flush_game_saves().await.expect("flush mode saves");

    let saved_games = service
        .load_games(user.id)
        .await
        .expect("reload mode saves");
    let mut restored = State::new_for_date(user.id, service, date, saved_games);
    restored.open_daily(0);
    assert_eq!(restored.board(), daily_board);
    assert_eq!(restored.moves(), 3);

    restored.show_personal();
    assert_eq!(restored.board(), personal_board);
    assert_eq!(restored.moves(), 8);
}

#[test]
fn personal_reset_restores_the_saved_scramble_from_its_seed() {
    let user_id = Uuid::now_v7();
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let seed = 0x5eed;
    let saved = personal_game(
        user_id,
        Difficulty::Easy,
        seed,
        vec![1, 2, 3, 4, 0, 5, 6, 7, 8],
        7,
    );
    let mut state = State::new_for_date(user_id, service(), date, vec![saved]);
    state.open_daily(0);
    state.show_personal();

    assert_eq!(state.moves(), 7);
    assert!(!state.request_reset());
    assert!(state.request_reset());
    state.reset();

    assert_eq!(
        state.board(),
        generate_scramble(Difficulty::Easy, seed).tiles
    );
    assert_eq!(state.scramble_seed(), seed);
    assert_eq!(state.moves(), 0);
}

#[test]
fn utc_rollover_regenerates_the_daily_slot_and_leaves_personal_progress_alone() {
    let user_id = Uuid::now_v7();
    let old_date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let personal_seed = 0x5eed;
    let personal_board = vec![1, 2, 3, 4, 0, 5, 6, 7, 8];
    let saved = personal_game(
        user_id,
        Difficulty::Easy,
        personal_seed,
        personal_board.clone(),
        7,
    );
    let mut state = State::new_for_date(user_id, service(), old_date, vec![saved]);

    state.open_daily(0);
    state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 0, 5, 6, 7, 8], 4);
    assert!(state.move_blank(Direction::Right));
    assert_eq!(state.moves(), 5);
    assert_eq!(state.first_unfinished_daily(), Some(0));

    state.show_personal();
    assert_eq!(state.moves(), 7);

    state.ensure_current_daily();

    let today = Utc::now().date_naive();
    let today_seed = super::state::daily_seed(today, Difficulty::Easy);
    state.show_daily();
    assert_eq!(state.scramble_seed(), today_seed);
    assert_eq!(
        state.board(),
        generate_scramble(Difficulty::Easy, today_seed).tiles
    );
    assert_eq!(state.moves(), 0);
    assert_eq!(state.first_unfinished_daily(), None);
    assert!(!state.has_unfinished_daily());

    state.show_personal();
    assert_eq!(state.scramble_seed(), personal_seed);
    assert_eq!(
        state.board(),
        personal_board
            .iter()
            .copied()
            .map(|tile| tile as u8)
            .collect::<Vec<_>>()
    );
    assert_eq!(state.moves(), 7);
}

#[tokio::test]
async fn personal_solve_persists_without_a_daily_win_or_activity_reward() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-personal-no-reward").await;
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new_for_date(user.id, service.clone(), date, Vec::new());
    state.show_personal();
    state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 5, 6, 7, 0, 8], 4);

    assert!(state.move_blank(Direction::Right));
    service
        .flush_game_saves()
        .await
        .expect("flush personal solve");

    let client = test_db.db.get().await.expect("db client");
    assert!(
        DailyWin::find(&client, user.id, Difficulty::Easy.key(), date)
            .await
            .expect("daily win lookup")
            .is_none()
    );
    assert!(matches!(
        activity_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    let saved = service
        .load_games(user.id)
        .await
        .expect("load personal save")
        .into_iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == Difficulty::Easy.key())
        .expect("saved personal easy game");
    assert_eq!(saved.puzzle_date, None);
    assert_eq!(saved.tiles, vec![1, 2, 3, 4, 5, 6, 7, 8, 0]);
    assert_eq!(saved.moves, 5);
}

#[tokio::test]
async fn finishing_move_composes_win_activity_and_solved_persistence_once() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-state-finish").await;
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new_for_date(user.id, service.clone(), date, Vec::new());
    state.set_board_for_test(Difficulty::Easy, vec![1, 2, 3, 4, 5, 6, 7, 0, 8], 4);

    assert!(state.move_blank(Direction::Right));
    service
        .flush_game_saves()
        .await
        .expect("flush completed move");
    assert!(state.is_solved());
    assert!(state.win_reported());
    assert_eq!(state.moves(), 5);

    let event = activity_rx.try_recv().expect("Sliding Puzzle win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::SlidingPuzzle,
            ref detail,
            score: Some(5),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(matches!(
        activity_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, Difficulty::Easy.key(), date)
        .await
        .expect("load daily win")
        .expect("daily win exists");
    assert_eq!(win.moves, 5);

    let saved = service
        .load_games(user.id)
        .await
        .expect("load saved games")
        .into_iter()
        .find(|game| game.difficulty_key == Difficulty::Easy.key())
        .expect("saved easy game");
    assert_eq!(saved.tiles, vec![1, 2, 3, 4, 5, 6, 7, 8, 0]);
    assert_eq!(saved.moves, 5);

    let solved = state.board().to_vec();
    assert!(!state.move_blank(Direction::Left));
    service
        .flush_game_saves()
        .await
        .expect("flush after ignored input");
    assert_eq!(state.board(), solved);
    assert_eq!(state.moves(), 5);
    assert!(state.win_reported());
    assert!(matches!(
        activity_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

/// Swapping two tiles of a solved board flips the puzzle's parity, so no
/// sequence of slides can ever reach it. A row like that is corrupt, not
/// progress, at every board dimension.
#[test]
fn unreachable_saved_daily_rows_regenerate_at_every_dimension() {
    let user_id = Uuid::now_v7();
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();

    for (index, difficulty) in Difficulty::ALL.iter().enumerate() {
        let mut tiles: Vec<i32> = solved_board(*difficulty)
            .into_iter()
            .map(i32::from)
            .collect();
        tiles.swap(0, 1);
        let unreachable = game(user_id, today, *difficulty, tiles, 12);

        let mut state = State::new_for_date(user_id, service(), today, vec![unreachable]);
        state.open_daily(index);

        let expected = generate_scramble(*difficulty, super::state::daily_seed(today, *difficulty));
        assert_eq!(
            state.board(),
            expected.tiles,
            "{} daily board should regenerate",
            difficulty.key()
        );
        assert_eq!(state.moves(), 0, "{}", difficulty.key());
    }
}

#[test]
fn unreachable_saved_personal_rows_are_discarded_before_activation() {
    let user_id = Uuid::now_v7();
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let mut tiles: Vec<i32> = solved_board(Difficulty::Easy)
        .into_iter()
        .map(i32::from)
        .collect();
    tiles.swap(0, 1);
    let unreachable = personal_game(user_id, Difficulty::Easy, 0x5eed, tiles.clone(), 9);

    let mut state = State::new_for_date(user_id, service(), date, vec![unreachable]);
    state.open_daily(0);
    state.show_personal();

    assert_ne!(
        state
            .board()
            .iter()
            .copied()
            .map(i32::from)
            .collect::<Vec<_>>(),
        tiles
    );
    assert_eq!(state.moves(), 0);
}

#[test]
fn stale_saved_rows_regenerate_today_deterministically() {
    let user_id = Uuid::now_v7();
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let stale = game(
        user_id,
        today - Duration::days(1),
        Difficulty::Medium,
        solved_board(Difficulty::Medium)
            .into_iter()
            .map(i32::from)
            .collect(),
        99,
    );

    let state = State::new_for_date(user_id, service(), today, vec![stale]);
    let expected = generate_scramble(
        Difficulty::Medium,
        super::state::daily_seed(today, Difficulty::Medium),
    );
    assert_eq!(state.board(), expected.tiles);
    assert_eq!(state.moves(), 0);
    assert!(!state.is_solved());
}
