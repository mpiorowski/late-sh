use late_core::models::twenty_forty_eight::{Game, HighScore};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn saves_and_loads_2048_game_state() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "2048-save-it").await;
    let client = test_db.db.get().await.expect("db client");

    Game::upsert(
        &client,
        user.id,
        128,
        serde_json::json!([[2, 4, 8, 16], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]),
        false,
    )
    .await
    .expect("save game");

    let saved = Game::find_by_user_id(&client, user.id)
        .await
        .expect("load game")
        .expect("existing game");

    assert_eq!(saved.score, 128);
    assert!(!saved.is_game_over);
    assert_eq!(saved.grid[0][0].as_u64(), Some(2));
}

#[tokio::test]
async fn high_score_only_moves_up() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "2048-high-score-it").await;
    let client = test_db.db.get().await.expect("db client");

    HighScore::update_score_if_higher(&client, user.id, 512)
        .await
        .expect("initial score");
    HighScore::update_score_if_higher(&client, user.id, 128)
        .await
        .expect("lower score should not replace");

    let saved = HighScore::find_by_user_id(&client, user.id)
        .await
        .expect("load score")
        .expect("existing score");

    assert_eq!(saved.score, 512);
}
