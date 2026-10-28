use chrono::NaiveDate;
use late_core::{
    chess_puzzle::ChessPuzzle,
    models::{chess_puzzle::DailyWin, chips::Difficulty},
    test_utils::create_test_user,
};
use tokio::sync::broadcast;

use super::{ChessPuzzleService, Daily};
use crate::{
    app::{
        activity::event::{ActivityEvent, ActivityGame, ActivityKind},
        arcade::chess_puzzle::state::{DIFFICULTIES, REPLY_DELAY_TICKS, State, find_puzzle},
    },
    test_helpers::new_test_db,
};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

/// Easy `02LIr`: after White's Kg2, Black mates with Qf2+ Kh3 Qxh2#.
fn mate_in_two() -> &'static ChessPuzzle {
    find_puzzle(Difficulty::Easy, "02LIr").expect("puzzle in the pack")
}

/// A day's claim as the service would hand it over.
fn daily(puzzle_date: NaiveDate) -> Daily {
    Daily {
        puzzle_date,
        puzzles: [
            find_puzzle(Difficulty::Easy, "3WAR5").expect("puzzle in the pack"),
            find_puzzle(Difficulty::Medium, "uHqD7").expect("puzzle in the pack"),
            find_puzzle(Difficulty::Hard, "IYCVq").expect("puzzle in the pack"),
        ],
    }
}

/// Every session of a day gets the puzzles the first one claimed, one
/// pack puzzle per tier.
#[tokio::test]
async fn the_day_is_claimed_once_and_every_session_gets_its_puzzles() {
    let test_db = new_test_db().await;
    let (activity_tx, _) = broadcast::channel(8);
    let service = ChessPuzzleService::new(test_db.db.clone(), activity_tx);

    let first = service.ensure_daily_puzzles().await.expect("claim");
    let second = service.ensure_daily_puzzles().await.expect("reuse");
    assert_eq!(first.puzzle_date, service.today());
    for (index, difficulty) in DIFFICULTIES.into_iter().enumerate() {
        assert_eq!(
            first.puzzles[index].id, second.puzzles[index].id,
            "{}",
            difficulty.key()
        );
        assert!(find_puzzle(difficulty, &first.puzzles[index].id).is_some());
    }
}

/// Play the puzzle through first try: c5 to f2, the reply, f2 to h2.
fn solve(state: &mut State) {
    state.click(34);
    state.click(13);
    for _ in 0..REPLY_DELAY_TICKS {
        state.tick();
    }
    state.click(13);
    state.click(15);
}

/// The first win of a day publishes one `GameWon` naming the tier; a
/// replay of the same board publishes nothing and keeps the fewer
/// mistakes.
#[tokio::test]
async fn a_fresh_win_publishes_once_and_a_replay_publishes_nothing() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-svc-win").await;
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = ChessPuzzleService::new(test_db.db.clone(), activity_tx);

    service
        .record_win_and_publish(user.id, Difficulty::Medium, date(), 2)
        .await
        .expect("first win");
    service
        .record_win_and_publish(user.id, Difficulty::Medium, date(), 0)
        .await
        .expect("replay");

    let event = activity_rx.recv().await.expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::ChessPuzzle,
            ref detail,
            score: Some(2),
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
    assert_eq!(win.mistakes, 0);
}

/// The finishing move of a daily records the win, publishes it, and saves
/// the solved board behind it, once.
#[tokio::test]
async fn the_finishing_move_of_a_daily_records_the_win_and_saves_the_board() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-svc-finish").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = ChessPuzzleService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new(user.id, service.clone(), Some(daily(date())), Vec::new());
    state.set_puzzle_for_test(Difficulty::Easy, mate_in_two());

    solve(&mut state);
    service.flush_game_saves().await.expect("flush");

    let event = activity_rx.try_recv().expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::ChessPuzzle,
            ref detail,
            score: Some(0),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(activity_rx.try_recv().is_err());
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, "easy", date())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.mistakes, 0);
    let saved = service
        .load_games(user.id)
        .await
        .expect("load games")
        .into_iter()
        .find(|game| game.mode == "daily" && game.difficulty_key == "easy")
        .expect("saved daily board");
    assert_eq!(
        (
            saved.puzzle_date,
            saved.puzzle_id.as_str(),
            saved.ply,
            saved.mistake_plies.clone(),
            saved.mate_move.clone()
        ),
        (Some(date()), "02LIr", 3, Vec::<i32>::new(), None)
    );
}

/// A personal puzzle saves its solve and earns nothing: no win row, no
/// event.
#[tokio::test]
async fn a_personal_finish_saves_without_a_win_or_an_event() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-svc-personal").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = ChessPuzzleService::new(test_db.db.clone(), activity_tx);
    let mut state = State::new(user.id, service.clone(), Some(daily(date())), Vec::new());
    state.show_personal();
    state.set_puzzle_for_test(Difficulty::Easy, mate_in_two());

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
    assert_eq!(
        (saved.puzzle_date, saved.puzzle_id.as_str(), saved.ply),
        (None, "02LIr", 3)
    );
}
