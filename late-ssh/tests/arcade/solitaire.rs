use late_core::models::solitaire::{DailyWin, Game, GameParams};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn saves_and_loads_solitaire_slots() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "solitaire-slots-it").await;
    let client = test_db.db.get().await.expect("db client");

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: "draw-1".to_string(),
            puzzle_date: Some(chrono::Utc::now().date_naive()),
            puzzle_seed: 123,
            stock: serde_json::json!([{ "suit": "Hearts", "rank": 1 }]),
            waste: serde_json::json!([{ "suit": "Spades", "rank": 13 }]),
            foundations: serde_json::json!([[], [], [], []]),
            tableau: serde_json::json!([
                [{ "card": { "suit": "Clubs", "rank": 7 }, "face_up": true }],
                [],
                [],
                [],
                [],
                [],
                []
            ]),
            is_game_over: false,
            score: 4,
        },
    )
    .await
    .expect("save solitaire game");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("load games");

    assert_eq!(games.len(), 1);
    assert_eq!(games[0].mode, "daily");
    assert_eq!(games[0].difficulty_key, "draw-1");
    assert_eq!(games[0].puzzle_seed, 123);
    assert_eq!(games[0].score, 4);
}

#[tokio::test]
async fn solitaire_daily_wins_keep_best_score() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "solitaire-win-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = chrono::Utc::now().date_naive();

    DailyWin::record_win(&client, user.id, "draw-3".to_string(), today, 12)
        .await
        .expect("initial win");
    DailyWin::record_win(&client, user.id, "draw-3".to_string(), today, 3)
        .await
        .expect("lower score should not replace");

    let wins = DailyWin::list_by_user_id(&client, user.id)
        .await
        .expect("load wins");

    assert_eq!(wins.len(), 1);
    assert_eq!(wins[0].difficulty_key, "draw-3");
    assert_eq!(wins[0].score, 12);
    assert!(
        DailyWin::has_won_today(&client, user.id, "draw-3", today)
            .await
            .expect("won today")
    );
}
