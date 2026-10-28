use chrono::NaiveDate;
use late_core::{
    models::{chips::Difficulty, word_wheel::DailyWin},
    test_utils::create_test_user,
};
use tokio::sync::broadcast;

use super::WordWheelService;
use crate::{
    app::{
        activity::event::{ActivityEvent, ActivityGame, ActivityKind},
        arcade::word_wheel::state::{State, find_wheel},
    },
    test_helpers::new_test_db,
};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

fn play(state: &mut State, word: &str) {
    for letter in word.chars() {
        state.push_letter(letter);
    }
    state.submit();
}

/// Every session of a day gets the wheel the first one claimed, and it is
/// a pack wheel.
#[tokio::test]
async fn the_day_is_claimed_once_and_every_session_gets_its_wheel() {
    let test_db = new_test_db().await;
    let (activity_tx, _) = broadcast::channel(8);
    let service = WordWheelService::new(test_db.db.clone(), activity_tx);

    let first = service.ensure_daily_wheel().await.expect("claim");
    let second = service.ensure_daily_wheel().await.expect("reuse");
    assert_eq!(
        (first.puzzle_date, second.wheel.letters.as_str()),
        (service.today(), first.wheel.letters.as_str())
    );
    assert!(find_wheel(&first.wheel.letters).is_some());
}

/// The first record of a rank publishes one `GameWon` naming the tier; a
/// repeat publishes nothing and keeps the higher score.
#[tokio::test]
async fn a_fresh_rank_publishes_once_and_a_repeat_publishes_nothing() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "word-wheel-svc-win").await;
    let (activity_tx, mut activity_rx) = broadcast::channel(8);

    super::record_win_and_publish(
        &test_db.db,
        &activity_tx,
        user.id,
        Difficulty::Medium,
        date(),
        50,
    )
    .await
    .expect("first record");
    super::record_win_and_publish(
        &test_db.db,
        &activity_tx,
        user.id,
        Difficulty::Medium,
        date(),
        61,
    )
    .await
    .expect("repeat");

    let event = activity_rx.recv().await.expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::WordWheel,
            ref detail,
            score: Some(50),
        } if detail.as_deref() == Some("medium")
    ));
    assert!(
        activity_rx.try_recv().is_err(),
        "a repeat must not publish another win"
    );
    let client = test_db.db.get().await.expect("db client");
    let win = DailyWin::find(&client, user.id, "medium", date())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(win.score, 61);
}

/// Words played on the day's wheel are saved, and the word that reaches
/// Bronze banks the easy win and publishes it.
#[tokio::test]
async fn play_saves_the_words_and_banks_the_rank_reached() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "word-wheel-svc-play").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = WordWheelService::new(test_db.db.clone(), activity_tx);
    let daily = super::Daily {
        puzzle_date: date(),
        wheel: find_wheel("rabcikt").expect("wheel in the pack"),
    };
    let mut state = State::new(user.id, service.clone(), Some(daily), None);

    for word in ["brickbat", "backtrack", "baccarat", "barbaric"] {
        play(&mut state, word);
    }
    service.flush_game_saves().await.expect("flush");

    let event = activity_rx.try_recv().expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::WordWheel,
            ref detail,
            score: Some(26),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(activity_rx.try_recv().is_err());
    let saved = service
        .load_game(user.id, date())
        .await
        .expect("load game")
        .expect("saved game");
    assert_eq!(
        (saved.letters.as_str(), saved.found),
        (
            "rabcikt",
            vec![
                "baccarat".to_string(),
                "backtrack".to_string(),
                "barbaric".to_string(),
                "brickbat".to_string(),
            ]
        )
    );
}

/// Two sessions of a day each stay under Bronze, but the words they save
/// merge past it. The next session restores the merged words and banks the
/// rank nobody banked: the merge must not leave a reached rank unpaid.
#[tokio::test]
async fn a_rank_reached_only_by_merged_sessions_is_banked_on_restore() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "word-wheel-svc-merge").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let service = WordWheelService::new(test_db.db.clone(), activity_tx);
    let daily = super::Daily {
        puzzle_date: date(),
        wheel: find_wheel("rabcikt").expect("wheel in the pack"),
    };

    let mut first = State::new(user.id, service.clone(), Some(daily), None);
    let mut second = State::new(user.id, service.clone(), Some(daily), None);
    for word in ["brickbat", "backtrack"] {
        play(&mut first, word);
    }
    for word in ["baccarat", "barbaric"] {
        play(&mut second, word);
    }
    service.flush_game_saves().await.expect("flush");
    assert!(
        activity_rx.try_recv().is_err(),
        "neither session reaches Bronze alone"
    );

    let saved = service.load_game(user.id, date()).await.expect("load game");
    let _third = State::new(user.id, service.clone(), Some(daily), saved);
    service.flush_game_saves().await.expect("flush");

    let event = activity_rx.try_recv().expect("win event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::WordWheel,
            ref detail,
            score: Some(26),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(activity_rx.try_recv().is_err());
}
