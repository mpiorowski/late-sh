use chrono::Utc;
use late_core::models::sudoku::{DailyWin, Game, GameParams};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

fn digit_grid(value: u8) -> serde_json::Value {
    serde_json::to_value([[value; 9]; 9]).expect("digit grid")
}

fn mask_grid(value: bool) -> serde_json::Value {
    serde_json::to_value([[value; 9]; 9]).expect("mask grid")
}

#[tokio::test]
async fn saves_daily_and_personal_sudoku_slots_separately() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sudoku-slots-it").await;
    let client = test_db.db.get().await.expect("db client");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: "medium".to_string(),
            puzzle_date: Some(Utc::now().date_naive()),
            puzzle_seed: 111,
            grid: digit_grid(1),
            fixed_mask: mask_grid(true),
            is_game_over: false,
            score: 0,
        },
    )
    .await
    .expect("save daily");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            difficulty_key: "medium".to_string(),
            puzzle_date: None,
            puzzle_seed: 222,
            grid: digit_grid(2),
            fixed_mask: mask_grid(false),
            is_game_over: false,
            score: 0,
        },
    )
    .await
    .expect("save personal");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("load games");

    assert_eq!(games.len(), 2);
    assert!(
        games
            .iter()
            .any(|game| game.mode == "daily" && game.puzzle_seed == 111)
    );
    assert!(
        games
            .iter()
            .any(|game| game.mode == "personal" && game.puzzle_seed == 222)
    );
}

#[tokio::test]
async fn upserting_same_sudoku_slot_updates_existing_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sudoku-upsert-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: "medium".to_string(),
            puzzle_date: Some(today),
            puzzle_seed: 111,
            grid: digit_grid(1),
            fixed_mask: mask_grid(true),
            is_game_over: false,
            score: 0,
        },
    )
    .await
    .expect("save daily");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: "medium".to_string(),
            puzzle_date: Some(today),
            puzzle_seed: 333,
            grid: digit_grid(3),
            fixed_mask: mask_grid(false),
            is_game_over: true,
            score: 7,
        },
    )
    .await
    .expect("update daily");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("load games");

    assert_eq!(games.len(), 1);
    assert_eq!(games[0].puzzle_seed, 333);
    assert!(games[0].is_game_over);
    assert_eq!(games[0].score, 7);
}

#[tokio::test]
async fn daily_win_is_recorded_and_detected_per_difficulty() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sudoku-win-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = Utc::now().date_naive();

    assert!(
        !DailyWin::has_won_today(&client, user.id, "medium", today)
            .await
            .expect("check pre-win state")
    );

    DailyWin::record_win(&client, user.id, "medium".to_string(), today, 1)
        .await
        .expect("record win");

    assert!(
        DailyWin::has_won_today(&client, user.id, "medium", today)
            .await
            .expect("check post-win state")
    );

    // Winning medium should not affect easy
    assert!(
        !DailyWin::has_won_today(&client, user.id, "easy", today)
            .await
            .expect("other difficulty should remain false")
    );
}
