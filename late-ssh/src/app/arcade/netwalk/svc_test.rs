use chrono::NaiveDate;
use late_core::{
    models::{chips::Difficulty, netwalk::DailyWin},
    test_utils::create_test_user,
};
use tokio::sync::broadcast;

use super::NetwalkService;
use crate::{
    app::{
        activity::event::{ActivityEvent, ActivityGame, ActivityKind},
        arcade::netwalk::state::{Side, State, Turn},
    },
    test_helpers::new_test_db,
};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

/// The comb board with its top-left leaf turned away; one counter-clockwise
/// turn there solves it.
fn one_turn_from_solved() -> Vec<u8> {
    // ╴ ╷ ╷ ╷ ╷ / │ x5 / ├ ┼ ┼ ┼ ┤ / │ x5 / ╵ x5
    let mut tiles = vec![4u8; 5];
    tiles[0] = 8;
    tiles.extend([5u8; 5]);
    tiles.extend([7u8, 15, 15, 15, 13]);
    tiles.extend([5u8; 5]);
    tiles.extend([1u8; 5]);
    tiles
}

/// Drive the cursor from the source to the top-left tile and turn it home.
fn solve(state: &mut State) {
    for side in [Side::West, Side::West, Side::North, Side::North] {
        assert!(state.move_cursor(side));
    }
    assert!(state.rotate_at_cursor(Turn::CounterClockwise));
    assert!(state.is_solved());
}

/// The first win of a day publishes one `GameWon` naming the board; a
/// replay of the same board publishes nothing and keeps the fewer moves.
#[tokio::test]
async fn a_fresh_win_publishes_once_and_a_replay_publishes_nothing() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "netwalk-svc-win").await;
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = NetwalkService::new(test_db.db.clone(), activity_tx);

    service
        .record_win_and_publish(user.id, Difficulty::Medium, date(), 30)
        .await
        .expect("first win");
    service
        .record_win_and_publish(user.id, Difficulty::Medium, date(), 21)
        .await
        .expect("replay");

    let event = activity_rx.recv().await.expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::Netwalk,
            ref detail,
            score: Some(30),
        } if detail.as_deref() == Some("medium")
    ));
    assert!(
        activity_rx.try_recv().is_err(),
        "a replay must not publish another win"
    );
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, "medium", date())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.moves, 21);
}

/// The finishing turn of a daily records the win, publishes it, and saves
/// the solved board behind it, once.
#[tokio::test]
async fn the_finishing_turn_of_a_daily_records_the_win_and_saves_the_board() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "netwalk-svc-finish").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = NetwalkService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new_for_date(user.id, service.clone(), date(), Vec::new());
    state.set_board_for_test(Difficulty::Easy, one_turn_from_solved(), 6);

    solve(&mut state);
    service.flush_game_saves().await.expect("flush");

    let event = activity_rx.try_recv().expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::Netwalk,
            ref detail,
            score: Some(7),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(activity_rx.try_recv().is_err());
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, "easy", date())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.moves, 7);
    let saved = service
        .load_games(user.id)
        .await
        .expect("load games")
        .into_iter()
        .find(|game| game.mode == "daily" && game.difficulty_key == "easy")
        .expect("saved daily board");
    assert_eq!(
        (saved.puzzle_date, saved.tiles.clone(), saved.moves),
        (
            Some(date()),
            state
                .board()
                .iter()
                .copied()
                .map(i32::from)
                .collect::<Vec<_>>(),
            7
        )
    );
}

/// A personal board saves its solve and earns nothing: no win row, no
/// event.
#[tokio::test]
async fn a_personal_finish_saves_without_a_win_or_an_event() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "netwalk-svc-personal").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = NetwalkService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new_for_date(user.id, service.clone(), date(), Vec::new());
    state.show_personal();
    state.set_board_for_test(Difficulty::Easy, one_turn_from_solved(), 2);

    solve(&mut state);
    service.flush_game_saves().await.expect("flush");

    assert!(activity_rx.try_recv().is_err());
    let client = test_db.db.get().await.expect("db client");
    assert!(
        DailyWin::find(&client, user.id, "easy", date())
            .await
            .expect("find win")
            .is_none()
    );
    let saved = service
        .load_games(user.id)
        .await
        .expect("load games")
        .into_iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == "easy")
        .expect("saved personal board");
    assert_eq!((saved.puzzle_date, saved.moves), (None, 3));
}
