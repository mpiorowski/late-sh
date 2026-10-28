use chrono::{Duration, NaiveDate};

use crate::{
    models::{
        chess_puzzle::{DailyPick, DailyWin, Game, GameParams},
        chips::Difficulty,
    },
    test_utils::{create_test_user, test_db},
};

fn params(user_id: uuid::Uuid, mode: &str, difficulty: Difficulty) -> GameParams {
    GameParams {
        user_id,
        mode: mode.to_string(),
        difficulty_key: difficulty.key().to_string(),
        puzzle_date: None,
        puzzle_id: "00008".to_string(),
        ply: 0,
        mistake_plies: Vec::new(),
        mate_move: None,
    }
}

/// The claim table answers the day's puzzle per tier and the last day each
/// puzzle was claimed for on that tier; a second insert for a date and
/// tier keeps the first claim.
#[tokio::test]
async fn daily_pick_keeps_the_first_claim_and_reports_last_use_per_tier() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap() + Duration::days(4000);

    DailyPick::insert_for_date(&**client, day - Duration::days(9), "easy", "3WAR5")
        .await
        .expect("old claim");
    let first = DailyPick::insert_for_date(&**client, day, "easy", "3WAR5")
        .await
        .expect("claim");
    let second = DailyPick::insert_for_date(&**client, day, "easy", "FDotv")
        .await
        .expect("racing claim");
    DailyPick::insert_for_date(&**client, day, "hard", "FDotv")
        .await
        .expect("hard claim");
    assert_eq!(
        (first.puzzle_id.as_str(), second.puzzle_id.as_str()),
        ("3WAR5", "3WAR5")
    );

    let mut found: Vec<(String, String)> = DailyPick::find_by_date(&**client, day)
        .await
        .expect("find claims")
        .into_iter()
        .map(|pick| (pick.difficulty_key, pick.puzzle_id))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("easy".to_string(), "3WAR5".to_string()),
            ("hard".to_string(), "FDotv".to_string()),
        ]
    );

    let easy = DailyPick::last_used(&**client, "easy")
        .await
        .expect("easy last used");
    assert_eq!(easy.get("3WAR5"), Some(&day));
    assert_eq!(
        easy.get("FDotv"),
        None,
        "another tier's claim is not this tier's"
    );
}

/// One daily and one personal slot per difficulty, each upsert replacing
/// its own slot only.
#[tokio::test]
async fn game_upsert_keeps_one_daily_and_one_personal_slot_per_difficulty() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-model-slots").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();

    for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
        Game::upsert(
            &client,
            GameParams {
                puzzle_date: Some(today - Duration::days(1)),
                ply: 2,
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
            puzzle_id: "0000D".to_string(),
            ply: 3,
            mistake_plies: vec![0, 2],
            ..params(user.id, "daily", Difficulty::Medium)
        },
    )
    .await
    .expect("update daily slot");
    Game::upsert(
        &client,
        GameParams {
            puzzle_id: "0009B".to_string(),
            ply: 1,
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
            daily.puzzle_id.as_str(),
            daily.ply,
            daily.mistake_plies.clone()
        ),
        (Some(today), "0000D", 3, vec![0, 2])
    );
    let personal = games
        .iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == "medium")
        .expect("medium personal slot");
    assert_eq!(
        (
            personal.puzzle_date,
            personal.puzzle_id.as_str(),
            personal.ply
        ),
        (None, "0009B", 1)
    );
}

/// Three lives: a fourth recorded mistake is a corrupt row, and the table
/// refuses it.
#[tokio::test]
async fn game_rows_hold_at_most_three_mistakes() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-model-lives").await;
    let client = test_db.db.get().await.expect("db client");

    let result = Game::upsert(
        &client,
        GameParams {
            mistake_plies: vec![0, 0, 2, 2],
            ..params(user.id, "personal", Difficulty::Easy)
        },
    )
    .await;
    assert!(result.is_err(), "four mistakes must be refused");
}

/// The first win is fresh and bumps the all-time rollup; a replay keeps the
/// fewest mistakes and is never fresh, so it neither pays nor counts again.
#[tokio::test]
async fn daily_win_keeps_the_fewest_mistakes_and_counts_once() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "chess-puzzle-model-win").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();

    let first = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 1)
        .await
        .expect("first win");
    let worse = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 2)
        .await
        .expect("worse replay");
    let clean = DailyWin::record_win(&client, user.id, Difficulty::Hard, today, 0)
        .await
        .expect("clean replay");
    assert_eq!(
        [
            (first.fresh, first.win.mistakes),
            (worse.fresh, worse.win.mistakes),
            (clean.fresh, clean.win.mistakes),
        ],
        [(true, 1), (false, 1), (false, 0)]
    );

    let found = DailyWin::find(&client, user.id, "hard", today)
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(found.mistakes, 0);

    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'chess_puzzle' AND user_id = $1",
            &[&user.id],
        )
        .await
        .expect("daily win total")
        .get(0);
    assert_eq!(total, 1);
}
