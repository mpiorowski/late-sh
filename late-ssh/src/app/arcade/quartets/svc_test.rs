use chrono::Utc;
use late_core::{
    models::quartets::{DailyWin, GameParams},
    test_utils::create_test_user,
};
use tokio::sync::broadcast;

use super::{Daily, QuartetsService, Today};
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
    let today = Utc::now().date_naive();

    let solved = || GameParams {
        user_id: user.id,
        puzzle_date: today,
        puzzle_id: 1,
        tries: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    };

    super::complete(&test_db.db, &activity_tx, solved(), 2)
        .await
        .expect("first record");
    super::complete(&test_db.db, &activity_tx, solved(), 0)
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
    let mut state = State::new(user.id, service.clone(), Some(Today { daily, saved: None }));

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

/// A solved wall is saved only behind its recorded win. When the win cannot
/// be written, the board is left one try short, so the player can enter the
/// solving try again on reconnect instead of finding a finished wall with no
/// win behind it.
#[tokio::test]
async fn a_win_that_fails_to_record_leaves_the_wall_unsolved() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quartets-svc-failed-win").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = QuartetsService::new(test_db.db.clone(), activity_tx);
    let today = Utc::now().date_naive();
    let daily = Daily {
        puzzle_date: today,
        puzzle: find_puzzle(1).expect("puzzle 1 in the pack"),
    };
    let client = test_db.db.get().await.expect("db client");
    client
        .batch_execute("ALTER TABLE quartets_daily_wins RENAME TO quartets_daily_wins_gone")
        .await
        .expect("break the win table");
    let mut state = State::new(user.id, service.clone(), Some(Today { daily, saved: None }));

    for words in [[0, 1, 2, 4], [0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]] {
        play(&mut state, words);
    }
    service.flush_game_saves().await.expect("flush");

    assert!(activity_rx.try_recv().is_err(), "no win was recorded");
    let saved = service
        .load_game(user.id, today)
        .await
        .expect("load game")
        .expect("saved game");
    assert_eq!(saved.tries, vec![0, 1, 2, 4, 0, 1, 2, 3, 4, 5, 6, 7]);
}

/// Two sessions on one wall: the first runs out of strikes, the second,
/// opened before that loss, played on and solved, so the loss bought a free
/// win. A lost wall stays lost: the second session's longer list does not
/// replace it, and its win is refused.
#[tokio::test]
async fn a_wall_lost_on_one_session_cannot_be_won_on_another() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quartets-svc-two-sessions").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = QuartetsService::new(test_db.db.clone(), activity_tx);
    let today = Utc::now().date_naive();
    let daily = Daily {
        puzzle_date: today,
        puzzle: find_puzzle(1).expect("puzzle 1 in the pack"),
    };
    let today_wall = || Today { daily, saved: None };
    let mut losing = State::new(user.id, service.clone(), Some(today_wall()));
    let mut winning = State::new(user.id, service.clone(), Some(today_wall()));

    for words in [[0, 1, 2, 4], [0, 1, 2, 5], [0, 1, 2, 6]] {
        play(&mut losing, words);
    }
    for words in [
        [0, 1, 2, 3],
        [4, 5, 6, 8],
        [4, 5, 6, 9],
        [4, 5, 6, 7],
        [8, 9, 10, 11],
    ] {
        play(&mut winning, words);
    }
    service.flush_game_saves().await.expect("flush");

    assert!(activity_rx.try_recv().is_err(), "the lost wall paid a win");
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, today)
        .await
        .expect("find win");
    let saved = service
        .load_game(user.id, today)
        .await
        .expect("load game")
        .expect("saved game");
    assert_eq!(
        (win.is_some(), saved.tries),
        (false, vec![0, 1, 2, 4, 0, 1, 2, 5, 0, 1, 2, 6])
    );
}
