use chrono::NaiveDate;
use late_core::{
    models::{
        minesweeper::{DailyWin, Game, GameParams},
        user::{User, UserParams},
    },
    test_utils::test_db,
};
use uuid::Uuid;

async fn create_user(client: &tokio_postgres::Client) -> Uuid {
    User::create(
        client,
        UserParams {
            fingerprint: format!("fp-{}", Uuid::now_v7()),
            username: "minesweeper-tester".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user")
    .id
}

#[tokio::test]
async fn upsert_and_find() {
    let tdb = test_db().await;
    let client = tdb.db.get().await.expect("client");
    let user_id = create_user(&client).await;

    let mine_map = serde_json::to_value(vec![
        vec![true, false, false],
        vec![false, false, false],
        vec![false, false, false],
    ])
    .unwrap();
    let player_grid = serde_json::to_value(vec![vec![0u8; 3]; 3]).unwrap();

    let game = Game::upsert(
        &client,
        GameParams {
            user_id,
            mode: "daily".to_string(),
            difficulty_key: "easy".to_string(),
            puzzle_date: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
            puzzle_seed: 42,
            mine_map: mine_map.clone(),
            player_grid: player_grid.clone(),
            lives: 3,
            is_game_over: false,
            score: 3,
        },
    )
    .await
    .expect("upsert");

    assert_eq!(game.user_id, user_id);
    assert_eq!(game.difficulty_key, "easy");
    assert_eq!(game.lives, 3);
    assert!(!game.is_game_over);

    let games = Game::list_by_user_id(&client, user_id).await.expect("find");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].puzzle_seed, 42);
}

#[tokio::test]
async fn upsert_overwrites_same_slot() {
    let tdb = test_db().await;
    let client = tdb.db.get().await.expect("client");
    let user_id = create_user(&client).await;

    let empty_grid = serde_json::to_value(vec![vec![0u8; 3]; 3]).unwrap();
    let empty_map = serde_json::to_value(vec![vec![false; 3]; 3]).unwrap();

    Game::upsert(
        &client,
        GameParams {
            user_id,
            mode: "daily".to_string(),
            difficulty_key: "easy".to_string(),
            puzzle_date: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
            puzzle_seed: 1,
            mine_map: empty_map.clone(),
            player_grid: empty_grid.clone(),
            lives: 3,
            is_game_over: false,
            score: 3,
        },
    )
    .await
    .expect("first upsert");

    let updated = Game::upsert(
        &client,
        GameParams {
            user_id,
            mode: "daily".to_string(),
            difficulty_key: "easy".to_string(),
            puzzle_date: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
            puzzle_seed: 2,
            mine_map: empty_map,
            player_grid: empty_grid,
            lives: 1,
            is_game_over: true,
            score: 1,
        },
    )
    .await
    .expect("second upsert");

    assert_eq!(updated.puzzle_seed, 2);
    assert_eq!(updated.lives, 1);
    assert!(updated.is_game_over);

    let games = Game::list_by_user_id(&client, user_id).await.expect("find");
    assert_eq!(games.len(), 1);
}

#[tokio::test]
async fn daily_win_record_and_check() {
    let tdb = test_db().await;
    let client = tdb.db.get().await.expect("client");
    let user_id = create_user(&client).await;
    let today = NaiveDate::from_ymd_opt(2026, 4, 2).unwrap();

    assert!(
        !DailyWin::has_won_today(&client, user_id, "easy", today)
            .await
            .expect("check")
    );

    let win = DailyWin::record_win(&client, user_id, "easy".to_string(), today, 2)
        .await
        .expect("record");
    assert_eq!(win.score, 2);

    assert!(
        DailyWin::has_won_today(&client, user_id, "easy", today)
            .await
            .expect("check")
    );

    assert!(
        !DailyWin::has_won_today(&client, user_id, "hard", today)
            .await
            .expect("check")
    );
}

#[tokio::test]
async fn daily_win_keeps_best_score() {
    let tdb = test_db().await;
    let client = tdb.db.get().await.expect("client");
    let user_id = create_user(&client).await;
    let today = NaiveDate::from_ymd_opt(2026, 4, 2).unwrap();

    DailyWin::record_win(&client, user_id, "medium".to_string(), today, 1)
        .await
        .expect("record");

    let win = DailyWin::record_win(&client, user_id, "medium".to_string(), today, 3)
        .await
        .expect("record");
    assert_eq!(win.score, 3);

    let win = DailyWin::record_win(&client, user_id, "medium".to_string(), today, 2)
        .await
        .expect("record");
    assert_eq!(win.score, 3);
}
