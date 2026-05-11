use chrono::Utc;
use late_core::models::nonogram::{DailyWin, Game, GameParams};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

fn player_grid(value: u8, width: usize, height: usize) -> serde_json::Value {
    serde_json::to_value(vec![vec![value; width]; height]).expect("player grid")
}

#[tokio::test]
async fn saves_daily_and_personal_nonogram_slots_per_size() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nonogram-slots-it").await;
    let client = test_db.db.get().await.expect("db client");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            size_key: "5x5".to_string(),
            puzzle_date: Some(Utc::now().date_naive()),
            puzzle_id: "5x5-000001".to_string(),
            player_grid: player_grid(1, 5, 5),
            is_game_over: false,
            score: 7,
        },
    )
    .await
    .expect("save daily");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            size_key: "5x5".to_string(),
            puzzle_date: None,
            puzzle_id: "5x5-000002".to_string(),
            player_grid: player_grid(0, 5, 5),
            is_game_over: false,
            score: 3,
        },
    )
    .await
    .expect("save personal");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            size_key: "10x10".to_string(),
            puzzle_date: None,
            puzzle_id: "10x10-000001".to_string(),
            player_grid: player_grid(1, 10, 10),
            is_game_over: true,
            score: 42,
        },
    )
    .await
    .expect("save second personal");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("load games");

    assert_eq!(games.len(), 3);
    assert!(
        games
            .iter()
            .any(|game| game.mode == "daily" && game.size_key == "5x5")
    );
    assert!(
        games
            .iter()
            .any(|game| game.mode == "personal" && game.size_key == "5x5")
    );
    assert!(
        games
            .iter()
            .any(|game| game.mode == "personal" && game.size_key == "10x10")
    );
}

#[tokio::test]
async fn upserting_same_nonogram_slot_updates_existing_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nonogram-upsert-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            size_key: "8x8".to_string(),
            puzzle_date: Some(today),
            puzzle_id: "8x8-000001".to_string(),
            player_grid: player_grid(0, 8, 8),
            is_game_over: false,
            score: 1,
        },
    )
    .await
    .expect("save daily");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            size_key: "8x8".to_string(),
            puzzle_date: Some(today),
            puzzle_id: "8x8-000003".to_string(),
            player_grid: player_grid(1, 8, 8),
            is_game_over: true,
            score: 17,
        },
    )
    .await
    .expect("update daily");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("load games");

    assert_eq!(games.len(), 1);
    assert_eq!(games[0].puzzle_id, "8x8-000003");
    assert!(games[0].is_game_over);
    assert_eq!(games[0].score, 17);
}

#[tokio::test]
async fn daily_win_is_recorded_and_detected_per_size() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nonogram-win-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();

    assert!(
        !DailyWin::has_won_today(&client, user.id, "5x5", today)
            .await
            .expect("check pre-win state")
    );

    DailyWin::record_win(&client, user.id, "5x5".to_string(), today)
        .await
        .expect("record win");

    assert!(
        DailyWin::has_won_today(&client, user.id, "5x5", today)
            .await
            .expect("check post-win state")
    );
    assert!(
        !DailyWin::has_won_today(&client, user.id, "8x8", today)
            .await
            .expect("other size should remain false")
    );
}
