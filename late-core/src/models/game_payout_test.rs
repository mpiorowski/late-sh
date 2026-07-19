use std::time::Duration;

use crate::{
    models::game_payout::{GamePayout, GamePayoutPeriodGrant},
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn daily_grant_credits_once_per_utc_day() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "daily-payout").await;
    let client = test_db.db.get().await.expect("db client");
    let today = chrono::Utc::now().date_naive();

    assert!(
        !GamePayout::has_claimed_daily(&client, user.id, "sudoku", "daily", today)
            .await
            .expect("check unclaimed day")
    );

    let first = GamePayout::grant_daily(
        &client,
        user.id,
        "sudoku",
        "daily",
        today,
        300,
        "sudoku_daily",
    )
    .await
    .expect("first daily payout");
    assert!(first.credited);
    assert_eq!(first.balance, 300);
    assert!(
        GamePayout::has_claimed_daily(&client, user.id, "sudoku", "daily", today)
            .await
            .expect("check claimed day")
    );

    let repeat = GamePayout::grant_daily(
        &client,
        user.id,
        "sudoku",
        "daily",
        today,
        300,
        "sudoku_daily",
    )
    .await
    .expect("repeat daily payout");
    assert!(!repeat.credited);
    assert_eq!(repeat.balance, 300);

    let tomorrow = today.succ_opt().expect("tomorrow");
    let next_day = GamePayout::grant_daily(
        &client,
        user.id,
        "sudoku",
        "daily",
        tomorrow,
        300,
        "sudoku_daily",
    )
    .await
    .expect("next-day payout");
    assert!(next_day.credited);
    assert_eq!(next_day.balance, 600);
}

#[tokio::test]
async fn period_grant_is_scoped_by_period_key() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "period-payout").await;
    let client = test_db.db.get().await.expect("db client");

    let grant = |period_key| GamePayoutPeriodGrant {
        user_id: user.id,
        game: "lateania",
        payout_kind: "boss",
        period_kind: "lifetime",
        period_key,
        amount: 1000,
        ledger_reason: "lateania_boss",
    };

    let first = GamePayout::grant_period(&client, grant("malgareth"))
        .await
        .expect("first boss payout");
    assert!(first.credited);
    assert_eq!(first.balance, 1000);
    assert!(
        GamePayout::has_claimed_period(
            &client,
            user.id,
            "lateania",
            "boss",
            "lifetime",
            "malgareth"
        )
        .await
        .expect("check claimed key")
    );
    assert!(
        !GamePayout::has_claimed_period(&client, user.id, "lateania", "boss", "lifetime", "king")
            .await
            .expect("check unclaimed key")
    );

    let repeat = GamePayout::grant_period(&client, grant("malgareth"))
        .await
        .expect("repeat boss payout");
    assert!(!repeat.credited);
    assert_eq!(repeat.balance, 1000);

    let other_key = GamePayout::grant_period(&client, grant("king"))
        .await
        .expect("second boss payout");
    assert!(other_key.credited);
    assert_eq!(other_key.balance, 2000);
}

#[tokio::test]
async fn cooldown_grant_records_claim_and_suppresses_repeat() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "cooldown-payout").await;
    let mut client = test_db.db.get().await.expect("db client");

    let first = GamePayout::grant_cooldown(
        &mut client,
        user.id,
        "chess",
        "win",
        Duration::from_secs(60 * 60),
        500,
        "chess_win",
    )
    .await
    .expect("first cooldown payout succeeds");

    assert!(first.credited);
    assert_eq!(first.balance, 500);

    let second = GamePayout::grant_cooldown(
        &mut client,
        user.id,
        "chess",
        "win",
        Duration::from_secs(60 * 60),
        500,
        "chess_win",
    )
    .await
    .expect("second cooldown payout succeeds");

    assert!(!second.credited);
    assert_eq!(second.balance, 500);

    let row = client
        .query_one(
            "SELECT count(*)::int AS claims, COALESCE(sum(amount), 0)::bigint AS amount
             FROM game_payout_claims
             WHERE user_id = $1
               AND game = 'chess'
               AND payout_kind = 'win'
               AND period_kind = 'cooldown'",
            &[&user.id],
        )
        .await
        .expect("query payout claims");
    assert_eq!(row.get::<_, i32>("claims"), 1);
    assert_eq!(row.get::<_, i64>("amount"), 500);

    let row = client
        .query_one(
            "SELECT count(*)::int AS rows, COALESCE(sum(delta), 0)::bigint AS delta
             FROM chip_ledger
             WHERE user_id = $1
               AND reason = 'chess_win'",
            &[&user.id],
        )
        .await
        .expect("query chip ledger");
    assert_eq!(row.get::<_, i32>("rows"), 1);
    assert_eq!(row.get::<_, i64>("delta"), 500);
}
