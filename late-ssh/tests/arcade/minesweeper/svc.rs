use late_core::models::minesweeper::{Game, GameParams};
use late_ssh::app::activity::event::ActivityEvent;
use late_ssh::app::arcade::chips::svc::ChipService;
use late_ssh::app::arcade::minesweeper::svc::MinesweeperService;
use tokio::sync::broadcast;

use super::super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn load_games_empty_for_new_user() {
    let tdb = new_test_db().await;
    let user_id = create_test_user(&tdb.db, "minesweeper-empty").await.id;

    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = MinesweeperService::new(tdb.db.clone(), tx, ChipService::new(tdb.db.clone()));

    let games = svc.load_games(user_id).await.expect("load");
    assert!(games.is_empty());
}

#[tokio::test]
async fn save_and_load_round_trip() {
    let tdb = new_test_db().await;
    let client = tdb.db.get().await.expect("client");
    let user_id = create_test_user(&tdb.db, "minesweeper-roundtrip").await.id;

    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = MinesweeperService::new(tdb.db.clone(), tx, ChipService::new(tdb.db.clone()));

    let mine_map = serde_json::to_value(vec![vec![false; 9]; 9]).unwrap();
    let player_grid = serde_json::to_value(vec![vec![0u8; 9]; 9]).unwrap();

    Game::upsert(
        &client,
        GameParams {
            user_id,
            mode: "daily".to_string(),
            difficulty_key: "medium".to_string(),
            puzzle_date: Some(svc.today()),
            puzzle_seed: 99,
            mine_map,
            player_grid,
            lives: 2,
            is_game_over: false,
            score: 2,
        },
    )
    .await
    .expect("upsert");

    let games = svc.load_games(user_id).await.expect("load");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].difficulty_key, "medium");
    assert_eq!(games[0].lives, 2);
}

#[tokio::test]
async fn daily_seed_is_deterministic() {
    let tdb = new_test_db().await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = MinesweeperService::new(tdb.db.clone(), tx, ChipService::new(tdb.db.clone()));

    let a = svc.get_daily_seed("easy");
    let b = svc.get_daily_seed("easy");
    assert_eq!(a, b);

    let c = svc.get_daily_seed("hard");
    assert_ne!(a, c);
}
