use chrono::{Duration, NaiveDate};

use crate::{
    models::{
        chips::Difficulty,
        sliding_puzzle::{DailyWin, Game, GameParams},
    },
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn game_upsert_keeps_daily_and_personal_slots_and_daily_win_keeps_best_moves() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-model-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();

    for (difficulty, positions) in [
        (Difficulty::Easy, 9i32),
        (Difficulty::Medium, 16),
        (Difficulty::Hard, 25),
    ] {
        Game::upsert(
            &client,
            GameParams {
                user_id: user.id,
                mode: "daily".to_string(),
                difficulty_key: difficulty.key().to_string(),
                puzzle_date: Some(today - Duration::days(1)),
                puzzle_seed: i64::from(positions),
                tiles: (0..positions).collect(),
                moves: 4,
            },
        )
        .await
        .expect("insert slot");
    }

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: Difficulty::Medium.key().to_string(),
            puzzle_date: Some(today),
            puzzle_seed: 16,
            tiles: (0..16).rev().collect(),
            moves: 17,
        },
    )
    .await
    .expect("update slot");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            difficulty_key: Difficulty::Medium.key().to_string(),
            puzzle_date: None,
            puzzle_seed: 99,
            tiles: (0..16).collect(),
            moves: 6,
        },
    )
    .await
    .expect("insert personal slot");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("list slots");
    assert_eq!(games.len(), 4);
    let medium = games
        .iter()
        .find(|game| game.mode == "daily" && game.difficulty_key == Difficulty::Medium.key())
        .expect("medium slot");
    assert_eq!(medium.puzzle_date, Some(today));
    assert_eq!(medium.moves, 17);
    let personal = games
        .iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == Difficulty::Medium.key())
        .expect("personal medium slot");
    assert_eq!(personal.puzzle_date, None);
    assert_eq!(personal.puzzle_seed, 99);
    assert_eq!(personal.moves, 6);

    let first = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 40)
        .await
        .expect("first win");
    let slower = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 52)
        .await
        .expect("slower replay");
    let faster = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 31)
        .await
        .expect("faster replay");
    assert!(first.fresh);
    assert!(!slower.fresh);
    assert!(!faster.fresh);
    assert_eq!(first.win.moves, 40);
    assert_eq!(slower.win.moves, 40);
    assert_eq!(faster.win.moves, 31);

    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'sliding_puzzle' AND user_id = $1",
            &[&user.id],
        )
        .await
        .expect("daily win total")
        .get(0);
    assert_eq!(total, 1);
}
