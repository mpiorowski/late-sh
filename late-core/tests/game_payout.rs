use std::time::Duration;

use late_core::{
    models::game_payout::GamePayout,
    test_utils::{create_test_user, test_db},
};

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
