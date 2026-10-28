use chrono::Utc;
use late_core::{models::quartets::DailyWin, test_utils::create_test_user};
use tokio::sync::broadcast;

use super::{Daily, QuartetsService};
use crate::{
    app::{
        activity::event::{ActivityEvent, ActivityGame, ActivityKind},
        arcade::quartets::state::{State, find_puzzle},
    },
    test_helpers::new_test_db,
};

fn play(state: &mut State, words: [u8; 4]) {
    for word in words {
        let position = state
            .wall()
            .iter()
            .position(|on_wall| *on_wall == word)
            .expect("word on the wall");
        state.click(position);
    }
    state.submit();
}

/// Every session of a day gets the wall the first one claimed, and it is
/// a pack puzzle.
#[tokio::test]
async fn the_day_is_claimed_once_and_every_session_gets_its_wall() {
    let test_db = new_test_db().await;
    let (activity_tx, _) = broadcast::channel(8);
    let service = QuartetsService::new(test_db.db.clone(), activity_tx);

    let first = service.ensure_daily_wall().await.expect("claim");
    let second = service.ensure_daily_wall().await.expect("reuse");
    assert_eq!(
        (first.puzzle_date, second.puzzle.id),
        (service.today(), first.puzzle.id)
    );
    assert!(find_puzzle(first.puzzle.id).is_some());
}

/// The first record of a solved wall publishes one `GameWon` on the
/// single board; a repeat publishes nothing and keeps the fewer strikes.
#[tokio::test]
async fn a_fresh_win_publishes_once_and_a_repeat_publishes_nothing() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quartets-svc-win").await;
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = QuartetsService::new(test_db.db.clone(), activity_tx);
    let today = Utc::now().date_naive();

    service
        .record_win_and_publish(user.id, today, 2)
        .await
        .expect("first record");
    service
        .record_win_and_publish(user.id, today, 0)
        .await
        .expect("repeat");

    let event = activity_rx.recv().await.expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::Quartets,
            ref detail,
            score: None,
        } if detail.as_deref() == Some("daily")
    ));
    assert!(
        activity_rx.try_recv().is_err(),
        "a repeat must not publish another win"
    );
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, today)
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.strikes, 0);
}

/// Every try is saved, and the try that solves the wall banks the win
/// with the strikes spent.
#[tokio::test]
async fn play_saves_the_tries_and_banks_the_solved_wall() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quartets-svc-play").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = QuartetsService::new(test_db.db.clone(), activity_tx);
    let today = Utc::now().date_naive();
    let daily = Daily {
        puzzle_date: today,
        puzzle: find_puzzle(1).expect("puzzle 1 in the pack"),
    };
    let mut state = State::new(user.id, service.clone(), Some(daily), None);

    for words in [[0, 1, 2, 4], [0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]] {
        play(&mut state, words);
    }
    service.flush_game_saves().await.expect("flush");

    assert!(matches!(
        activity_rx.try_recv().expect("win event").kind,
        ActivityKind::GameWon {
            game: ActivityGame::Quartets,
            ..
        }
    ));
    assert!(activity_rx.try_recv().is_err());
    let saved = service
        .load_game(user.id, today)
        .await
        .expect("load game")
        .expect("saved game");
    assert_eq!(
        (saved.puzzle_id, saved.tries),
        (1, vec![0, 1, 2, 4, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11])
    );
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, today)
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.strikes, 1);
}
