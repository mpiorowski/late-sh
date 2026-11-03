use chrono::{Duration, NaiveDate};

use crate::{
    models::{
        chips::Difficulty,
        netwalk::{DailyWin, Game, GameParams},
    },
    test_utils::{create_test_user, test_db},
};

fn params(user_id: uuid::Uuid, mode: &str, difficulty: Difficulty) -> GameParams {
    GameParams {
        user_id,
        mode: mode.to_string(),
        difficulty_key: difficulty.key().to_string(),
        puzzle_date: None,
        puzzle_seed: 1,
        tiles: vec![10; 25],
        moves: 0,
    }
}

/// One daily and one personal slot per difficulty, each upsert replacing
/// its own slot only.
#[tokio::test]
async fn game_upsert_keeps_one_daily_and_one_personal_slot_per_difficulty() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "netwalk-model-slots").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();

    for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
        Game::upsert(
            &client,
            GameParams {
                puzzle_date: Some(today - Duration::days(1)),
                moves: 4,
                ..params(user.id, "daily", difficulty)
            },
        )
        .await
        .expect("insert daily slot");
    }
    Game::upsert(
        &client,
        GameParams {
            puzzle_date: Some(today),
            puzzle_seed: 7,
            tiles: vec![5; 49],
            moves: 17,
            ..params(user.id, "daily", Difficulty::Medium)
        },
    )
    .await
    .expect("update daily slot");
    Game::upsert(
        &client,
        GameParams {
            puzzle_seed: 99,
            moves: 6,
            ..params(user.id, "personal", Difficulty::Medium)
        },
    )
    .await
    .expect("insert personal slot");

    let games = Game::list_by_user_id(&client, user.id)
        .await
        .expect("list slots");
    assert_eq!(games.len(), 4);
    let daily = games
        .iter()
        .find(|game| game.mode == "daily" && game.difficulty_key == "medium")
        .expect("medium daily slot");
    assert_eq!(
        (
            daily.puzzle_date,
            daily.puzzle_seed,
            daily.tiles.clone(),
            daily.moves
        ),
        (Some(today), 7, vec![5; 49], 17)
    );
    let personal = games
        .iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == "medium")
        .expect("medium personal slot");
    assert_eq!(
        (personal.puzzle_date, personal.puzzle_seed, personal.moves),
        (None, 99, 6)
    );
}

/// The first win is fresh and bumps the all-time rollup; a replay keeps the
/// fewest moves and is never fresh, so it neither pays nor counts again.
#[tokio::test]
async fn daily_win_keeps_the_fewest_moves_and_counts_once() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "netwalk-model-win").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();

    let first = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 40)
        .await
        .expect("first win");
    let slower = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 52)
        .await
        .expect("slower replay");
    let faster = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 31)
        .await
        .expect("faster replay");
    assert_eq!(
        [
            (first.fresh, first.win.moves),
            (slower.fresh, slower.win.moves),
            (faster.fresh, faster.win.moves),
        ],
        [(true, 40), (false, 40), (false, 31)]
    );

    let found = DailyWin::find(&client, user.id, "hard", today)
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(found.moves, 31);

    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'netwalk' AND user_id = $1",
            &[&user.id],
        )
        .await
        .expect("daily win total")
        .get(0);
    assert_eq!(total, 1);
}
