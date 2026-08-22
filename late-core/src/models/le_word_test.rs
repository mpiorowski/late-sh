use crate::{
    models::le_word::{DailyWin, DailyWord, Game, GameParams, ReplayGame, ReplayGameParams},
    test_utils::{create_test_user, test_db},
};
use chrono::NaiveDate;

#[tokio::test]
async fn daily_word_records_one_global_answer_per_date() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("client");
    let tx = client.transaction().await.expect("transaction");
    let today = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

    let inserted = DailyWord::insert_for_date(&*tx, today, "hunch")
        .await
        .expect("insert daily word");
    assert_eq!(inserted.answer_word, "hunch");

    let again = DailyWord::insert_for_date(&*tx, today, "glass")
        .await
        .expect("same date keeps existing answer");
    assert_eq!(again.answer_word, "hunch");

    let found = DailyWord::find_by_date(&*tx, today)
        .await
        .expect("find daily word")
        .expect("daily word exists");
    assert_eq!(found.answer_word, "hunch");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn game_progress_and_daily_win_persist() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "le-word-player").await;
    let today = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            puzzle_date: today,
            answer_word: "hunch".to_string(),
            guesses: serde_json::json!(["glass", "hunch"]),
            current_guess: String::new(),
            is_game_over: true,
            won: true,
        },
    )
    .await
    .expect("save game");

    let game = Game::find_by_user_id_for_date(&client, user.id, today)
        .await
        .expect("load game")
        .expect("game exists");
    assert_eq!(game.answer_word, "hunch");
    assert!(game.won);

    assert!(
        !DailyWin::has_won_today(&client, user.id, today)
            .await
            .expect("initial win check")
    );

    DailyWin::record_win(&client, user.id, today, 2)
        .await
        .expect("record win");
    let win = DailyWin::record_win(&client, user.id, today, 4)
        .await
        .expect("record worse win");
    assert_eq!(win.score, 2);
    assert!(
        DailyWin::has_won_today(&client, user.id, today)
            .await
            .expect("win check")
    );
}

#[tokio::test]
async fn daily_and_replay_progress_use_separate_slots() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "le-word-replay-player").await;
    let today = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            puzzle_date: today,
            answer_word: "hunch".to_string(),
            guesses: serde_json::json!(["glass"]),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
        },
    )
    .await
    .expect("save daily game");
    ReplayGame::upsert(
        &client,
        ReplayGameParams {
            user_id: user.id,
            answer_word: "apple".to_string(),
            guesses: serde_json::json!([]),
            current_guess: "sh".to_string(),
            is_game_over: false,
            won: false,
        },
    )
    .await
    .expect("save replay game");
    ReplayGame::upsert(
        &client,
        ReplayGameParams {
            user_id: user.id,
            answer_word: "shade".to_string(),
            guesses: serde_json::json!(["apple"]),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
        },
    )
    .await
    .expect("replace replay game");

    let daily = Game::find_by_user_id_for_date(&client, user.id, today)
        .await
        .expect("load daily game")
        .expect("saved daily game");
    let replay = ReplayGame::find_by_user_id(&client, user.id)
        .await
        .expect("load replay game")
        .expect("saved replay game");
    assert_eq!(daily.puzzle_date, today);
    assert_eq!(daily.answer_word, "hunch");
    assert_eq!(replay.answer_word, "shade");
}
