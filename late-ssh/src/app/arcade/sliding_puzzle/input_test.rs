use chrono::NaiveDate;
use late_core::{
    db::{Db, DbConfig},
    models::chips::Difficulty,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    image::TileView,
    input::{handle_arrow, handle_key},
    state::{Mode, State},
    svc::SlidingPuzzleService,
};
use crate::app::activity::event::ActivityEvent;

const DATE: (i32, u32, u32) = (2026, 8, 21);
const CENTER_BLANK: [u8; 9] = [1, 2, 3, 4, 0, 5, 6, 7, 8];

fn service() -> SlidingPuzzleService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    SlidingPuzzleService::new(db, activity)
}

fn state_with_board(board: Vec<u8>) -> State {
    let date = NaiveDate::from_ymd_opt(DATE.0, DATE.1, DATE.2).unwrap();
    let mut state = State::new_for_date(Uuid::now_v7(), service(), date, Vec::new());
    state.set_board_for_test(Difficulty::Easy, board, 0);
    state
}

#[test]
fn sliding_puzzle_lowercase_and_uppercase_i_toggle_image_tiles() {
    let mut state = state_with_board(CENTER_BLANK.to_vec());
    assert_eq!(state.tile_view(), TileView::Numbered);

    assert!(handle_key(&mut state, b'i'));
    assert_eq!(state.tile_view(), TileView::Image);
    assert_eq!(state.board(), CENTER_BLANK);
    assert_eq!(state.moves(), 0);

    assert!(handle_key(&mut state, b'I'));
    assert_eq!(state.tile_view(), TileView::Numbered);
    assert_eq!(state.board(), CENTER_BLANK);
    assert_eq!(state.moves(), 0);
}

#[test]
fn arrows_and_hjkl_slide_a_tile_in_the_requested_direction() {
    let cases = [
        (b'k', b'A', vec![1, 2, 3, 4, 7, 5, 6, 0, 8]),
        (b'j', b'B', vec![1, 0, 3, 4, 2, 5, 6, 7, 8]),
        (b'l', b'C', vec![1, 2, 3, 0, 4, 5, 6, 7, 8]),
        (b'h', b'D', vec![1, 2, 3, 4, 5, 0, 6, 7, 8]),
    ];

    for (key, arrow, expected) in cases {
        for key in [key, key.to_ascii_uppercase()] {
            let mut state = state_with_board(CENTER_BLANK.to_vec());
            assert!(handle_key(&mut state, key));
            assert_eq!(state.board(), expected, "keyboard route {key:?}");
            assert_eq!(state.moves(), 1);
        }

        let mut state = state_with_board(CENTER_BLANK.to_vec());
        assert!(handle_arrow(&mut state, arrow));
        assert_eq!(state.board(), expected, "arrow route {arrow:?}");
        assert_eq!(state.moves(), 1);
    }
}

#[test]
fn difficulty_and_reset_keys_follow_daily_puzzle_routing() {
    let date = NaiveDate::from_ymd_opt(DATE.0, DATE.1, DATE.2).unwrap();
    let mut state = State::new_for_date(Uuid::now_v7(), service(), date, Vec::new());
    assert_eq!(state.difficulty(), Difficulty::Medium);

    assert!(handle_key(&mut state, b']'));
    assert_eq!(state.difficulty(), Difficulty::Hard);
    assert!(handle_key(&mut state, b']'));
    assert_eq!(state.difficulty(), Difficulty::Easy);
    assert!(handle_key(&mut state, b'['));
    assert_eq!(state.difficulty(), Difficulty::Hard);

    let scramble = state.board().to_vec();
    assert!(handle_key(&mut state, b'R'));
    assert!(state.reset_pending());
    assert!(handle_key(&mut state, b'0'));
    assert!(!state.reset_pending());
    assert_eq!(state.board(), scramble);
    assert_eq!(state.moves(), 0);
    assert!(!handle_key(&mut state, b'?'));
}

#[test]
fn legal_edge_routes_are_consumed_without_mutating_the_board() {
    let board = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    for key in *b"jJlL" {
        let mut state = state_with_board(board.clone());
        assert!(handle_key(&mut state, key));
        assert_eq!(state.board(), board);
        assert_eq!(state.moves(), 0);
    }
    for arrow in *b"BC" {
        let mut state = state_with_board(board.clone());
        assert!(handle_arrow(&mut state, arrow));
        assert_eq!(state.board(), board);
        assert_eq!(state.moves(), 0);
    }
}

#[tokio::test]
async fn daily_personal_and_new_keys_route_replay_modes() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user =
        late_core::test_utils::create_test_user(&test_db.db, "sliding-puzzle-mode-input").await;
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity);
    let date = NaiveDate::from_ymd_opt(DATE.0, DATE.1, DATE.2).unwrap();
    let mut state = State::new_for_date(user.id, service.clone(), date, Vec::new());

    assert!(handle_key(&mut state, b'p'));
    assert_eq!(state.mode, Mode::Personal);
    let first_seed = state.scramble_seed();

    assert!(handle_key(&mut state, b'D'));
    assert_eq!(state.mode, Mode::Daily);
    assert!(handle_key(&mut state, b'P'));
    assert_eq!(state.scramble_seed(), first_seed);

    assert!(handle_key(&mut state, b'n'));
    assert!(state.reset_pending());
    assert!(handle_key(&mut state, b'R'));
    assert!(state.reset_pending());
    assert_eq!(state.scramble_seed(), first_seed);
    assert!(handle_key(&mut state, b'0'));
    assert!(!state.reset_pending());
    assert_eq!(state.scramble_seed(), first_seed);

    assert!(handle_key(&mut state, b'n'));
    assert!(state.reset_pending());
    assert!(handle_key(&mut state, b'N'));
    assert_eq!(state.mode, Mode::Personal);
    assert_ne!(state.scramble_seed(), first_seed);
    assert_eq!(state.moves(), 0);
    service
        .flush_game_saves()
        .await
        .expect("flush personal board saves");
}
