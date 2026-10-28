use chrono::{Duration, NaiveDate};

use crate::{
    models::{
        chips::Difficulty,
        word_wheel::{DailyWheel, DailyWin, Game, GameParams},
    },
    test_utils::{create_test_user, test_db},
};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
}

fn params(user_id: uuid::Uuid, letters: &str, found: &[&str]) -> GameParams {
    GameParams {
        user_id,
        puzzle_date: today(),
        letters: letters.to_string(),
        found: found.iter().map(|word| word.to_string()).collect(),
    }
}

/// Two sessions saving the same wheel keep each other's words: a save
/// merges into the stored list, deduplicated and alphabetical. A save for
/// other letters replaces the list.
#[tokio::test]
async fn game_upsert_merges_the_words_of_the_same_wheel() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "word-wheel-model-merge").await;
    let client = test_db.db.get().await.expect("db client");

    Game::upsert(&client, params(user.id, "racdeit", &["tired", "cart"]))
        .await
        .expect("first session");
    let merged = Game::upsert(&client, params(user.id, "racdeit", &["arid", "tired"]))
        .await
        .expect("second session");
    assert_eq!(merged.found, ["arid", "cart", "tired"]);

    let replaced = Game::upsert(&client, params(user.id, "tacdeir", &["edict"]))
        .await
        .expect("other letters");
    assert_eq!(
        (replaced.letters.as_str(), replaced.found),
        ("tacdeir", vec!["edict".to_string()])
    );
    let stored = Game::find_for_date(&client, user.id, today())
        .await
        .expect("find game")
        .expect("game exists");
    assert_eq!(stored.found, ["edict"]);
}

/// The claim table answers the day's wheel and the last day each wheel
/// was claimed for; a second insert for a date keeps the first claim.
#[tokio::test]
async fn daily_wheel_keeps_the_first_claim_and_reports_last_use() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let day = today() + Duration::days(4000);

    DailyWheel::insert_for_date(&**client, day - Duration::days(9), "racdeit")
        .await
        .expect("old claim");
    let first = DailyWheel::insert_for_date(&**client, day, "racdeit")
        .await
        .expect("claim");
    let second = DailyWheel::insert_for_date(&**client, day, "tacdeir")
        .await
        .expect("racing claim");
    assert_eq!(
        (first.letters.as_str(), second.letters.as_str()),
        ("racdeit", "racdeit")
    );
    let found = DailyWheel::find_by_date(&**client, day)
        .await
        .expect("find claim")
        .expect("claim exists");
    assert_eq!(found.letters, "racdeit");
    let last_used = DailyWheel::last_used(&**client).await.expect("last used");
    assert_eq!(last_used.get("racdeit"), Some(&day));
    assert_eq!(last_used.get("tacdeir"), None);
}

/// Each rank is its own win: the first record is fresh and counts toward
/// the all-time rollup, a repeat keeps the higher score and is not fresh.
#[tokio::test]
async fn daily_win_counts_each_rank_once() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "word-wheel-model-win").await;
    let client = test_db.db.get().await.expect("db client");

    let bronze = DailyWin::record_win(&client, user.id, Difficulty::Easy, today(), 21)
        .await
        .expect("bronze");
    let silver = DailyWin::record_win(&client, user.id, Difficulty::Medium, today(), 48)
        .await
        .expect("silver");
    let again = DailyWin::record_win(&client, user.id, Difficulty::Easy, today(), 30)
        .await
        .expect("bronze again");
    assert_eq!(
        [
            (bronze.fresh, bronze.win.score),
            (silver.fresh, silver.win.score),
            (again.fresh, again.win.score),
        ],
        [(true, 21), (true, 48), (false, 30)]
    );
    let stored = DailyWin::find(&client, user.id, "easy", today())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(stored.score, 30);

    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'word_wheel' AND user_id = $1",
            &[&user.id],
        )
        .await
        .expect("daily win total")
        .get(0);
    assert_eq!(total, 2);
}
