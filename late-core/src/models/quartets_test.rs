use chrono::{Duration, NaiveDate};

use crate::{
    models::quartets::{DailyWall, DailyWin, Game, GameParams},
    test_utils::{create_test_user, test_db},
};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
}

fn params(user_id: uuid::Uuid, puzzle_id: i32, tries: &[i16]) -> GameParams {
    GameParams {
        user_id,
        puzzle_date: today(),
        puzzle_id,
        tries: tries.to_vec(),
    }
}

async fn stored(client: &tokio_postgres::Client, user_id: uuid::Uuid) -> Option<(i32, Vec<i16>)> {
    Game::find_for_date(client, user_id, today())
        .await
        .expect("find game")
        .map(|game| (game.puzzle_id, game.tries))
}

/// A save with more tries replaces the stored ones; a stale session's
/// shorter list does not, so it cannot hand back strikes. A save for
/// another puzzle replaces the row whatever its length.
#[tokio::test]
async fn game_upsert_keeps_the_longer_tries_of_the_same_puzzle() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "quartets-model-upsert").await;
    let client = test_db.db.get().await.expect("db client");
    Game::upsert(&**client, params(user.id, 7, &[0, 1, 2, 4]))
        .await
        .expect("first try");
    Game::upsert(&**client, params(user.id, 7, &[0, 1, 2, 4, 0, 1, 2, 3]))
        .await
        .expect("second try");
    assert_eq!(
        stored(&client, user.id).await,
        Some((7, vec![0, 1, 2, 4, 0, 1, 2, 3]))
    );

    Game::upsert(&**client, params(user.id, 7, &[4, 5, 6, 7]))
        .await
        .expect("stale session");
    assert_eq!(
        stored(&client, user.id).await,
        Some((7, vec![0, 1, 2, 4, 0, 1, 2, 3]))
    );

    Game::upsert(&**client, params(user.id, 9, &[]))
        .await
        .expect("other puzzle");
    assert_eq!(stored(&client, user.id).await, Some((9, vec![])));
}

/// The claim table answers the day's puzzle and the last day each puzzle
/// was claimed for; a second insert for a date keeps the first claim.
#[tokio::test]
async fn daily_wall_keeps_the_first_claim_and_reports_last_use() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let day = today() + Duration::days(4000);

    DailyWall::insert_for_date(&**client, day - Duration::days(9), 3)
        .await
        .expect("old claim");
    let first = DailyWall::insert_for_date(&**client, day, 3)
        .await
        .expect("claim");
    let second = DailyWall::insert_for_date(&**client, day, 5)
        .await
        .expect("racing claim");
    assert_eq!((first.puzzle_id, second.puzzle_id), (3, 3));
    let found = DailyWall::find_by_date(&**client, day)
        .await
        .expect("find claim")
        .expect("claim exists");
    assert_eq!(found.puzzle_id, 3);
    let last_used = DailyWall::last_used(&**client).await.expect("last used");
    assert_eq!(last_used.get(&3), Some(&day));
    assert_eq!(last_used.get(&5), None);
}

/// The first solve of a day is fresh and counts toward the all-time
/// rollup; a repeat keeps the fewer strikes and is not fresh.
#[tokio::test]
async fn daily_win_counts_once_and_keeps_the_fewer_strikes() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "quartets-model-win").await;
    let client = test_db.db.get().await.expect("db client");

    let first = DailyWin::record_win(&**client, user.id, today(), 2)
        .await
        .expect("first solve");
    let again = DailyWin::record_win(&**client, user.id, today(), 1)
        .await
        .expect("solve again");
    assert_eq!(
        [
            (first.fresh, first.win.strikes),
            (again.fresh, again.win.strikes)
        ],
        [(true, 2), (false, 1)]
    );
    let stored = DailyWin::find(&client, user.id, today())
        .await
        .expect("find win")
        .expect("win exists");
    assert_eq!(stored.strikes, 1);

    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'quartets' AND user_id = $1",
            &[&user.id],
        )
        .await
        .expect("daily win total")
        .get(0);
    assert_eq!(total, 1);
}
