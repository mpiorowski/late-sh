use std::time::Duration;

use crate::{
    models::chips::ChipMove,
    models::game_payout::{
        GAME_PAYOUT_PERIOD_COOLDOWN, GamePayout, GamePayoutKey, GamePayoutMultiGrant,
        GamePayoutPeriodGrant,
    },
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
        ChipMove::DailyPuzzleWin,
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
        ChipMove::DailyPuzzleWin,
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
        ChipMove::DailyPuzzleWin,
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
        chip_move: ChipMove::LateaniaArchdemonDefeat,
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
        ChipMove::DailyChessWin,
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
        ChipMove::DailyChessWin,
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
               AND reason = 'daily_chess_win'",
            &[&user.id],
        )
        .await
        .expect("query chip ledger");
    assert_eq!(row.get::<_, i32>("rows"), 1);
    assert_eq!(row.get::<_, i64>("delta"), 500);
}

// ---- the multi-key grant (SHOP.md Phase 6) -------------------------------

const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn run_grant<'a>(user_id: uuid::Uuid, keys: &'a [GamePayoutKey<'a>]) -> GamePayoutMultiGrant<'a> {
    GamePayoutMultiGrant {
        user_id,
        game: "nethack",
        payout_kind: "ascension",
        keys,
        amount: 50_000,
        chip_move: ChipMove::NethackAscension,
    }
}

fn run_keys<'a>(event_key: &'a str) -> [GamePayoutKey<'a>; 2] {
    [
        GamePayoutKey::Unique {
            period_kind: "event",
            period_key: event_key,
        },
        GamePayoutKey::Cooldown {
            period_kind: GAME_PAYOUT_PERIOD_COOLDOWN,
            window: WEEK,
        },
    ]
}

async fn claim_rows(client: &tokio_postgres::Client, user_id: uuid::Uuid) -> i64 {
    client
        .query_one(
            "SELECT COUNT(*)::bigint AS n FROM game_payout_claims WHERE user_id = $1",
            &[&user_id],
        )
        .await
        .expect("count claims")
        .get("n")
}

async fn ledger_rows(client: &tokio_postgres::Client, user_id: uuid::Uuid) -> (i64, i64) {
    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint AS n, COALESCE(SUM(delta), 0)::bigint AS delta
             FROM chip_ledger WHERE user_id = $1",
            &[&user_id],
        )
        .await
        .expect("count ledger");
    (row.get("n"), row.get("delta"))
}

#[tokio::test]
async fn multi_grant_writes_every_key_and_one_ledger_row() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "multi-payout").await;
    let mut client = test_db.db.get().await.expect("db client");

    let keys = run_keys("xlogfile:900");
    let grant = GamePayout::grant_multi(&mut client, run_grant(user.id, &keys))
        .await
        .expect("first run pays");
    assert!(grant.credited);
    assert_eq!(grant.balance, 50_000);

    assert_eq!(claim_rows(&client, user.id).await, 2);
    assert_eq!(ledger_rows(&client, user.id).await, (1, 50_000));
}

#[tokio::test]
async fn multi_grant_refuses_a_replayed_event_key() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "multi-replay").await;
    let mut client = test_db.db.get().await.expect("db client");

    let keys = run_keys("xlogfile:900");
    GamePayout::grant_multi(&mut client, run_grant(user.id, &keys))
        .await
        .expect("first run pays");

    // Even with the window walked past, the same line never pays twice.
    client
        .execute(
            "UPDATE game_payout_claims SET created = created - interval '30 days'
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("age claims");

    let repeat = GamePayout::grant_multi(&mut client, run_grant(user.id, &keys))
        .await
        .expect("replay is answered");
    assert!(!repeat.credited);
    assert_eq!(repeat.balance, 50_000);
    assert_eq!(claim_rows(&client, user.id).await, 2);
    assert_eq!(ledger_rows(&client, user.id).await, (1, 50_000));
}

#[tokio::test]
async fn multi_grant_refuses_a_fresh_event_inside_the_window_and_pays_after_it() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "multi-window").await;
    let mut client = test_db.db.get().await.expect("db client");

    let first = run_keys("xlogfile:900");
    GamePayout::grant_multi(&mut client, run_grant(user.id, &first))
        .await
        .expect("first run pays");

    // A different run, same week: the lockout refuses it, and refusing writes
    // nothing at all, so the event key stays free.
    let second = run_keys("xlogfile:1800");
    let inside = GamePayout::grant_multi(&mut client, run_grant(user.id, &second))
        .await
        .expect("second run is answered");
    assert!(!inside.credited);
    assert_eq!(inside.balance, 50_000);
    assert_eq!(claim_rows(&client, user.id).await, 2);
    assert_eq!(ledger_rows(&client, user.id).await, (1, 50_000));

    client
        .execute(
            "UPDATE game_payout_claims SET created = created - interval '8 days'
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("age claims");

    let outside = GamePayout::grant_multi(&mut client, run_grant(user.id, &second))
        .await
        .expect("third attempt pays");
    assert!(outside.credited);
    assert_eq!(outside.balance, 100_000);
    assert_eq!(claim_rows(&client, user.id).await, 4);
    assert_eq!(ledger_rows(&client, user.id).await, (2, 100_000));
}

#[tokio::test]
async fn a_legacy_lifetime_claim_never_blocks_the_first_repeat() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "multi-legacy").await;
    let mut client = test_db.db.get().await.expect("db client");

    // What every door milestone banked before Phase 6: one 'lifetime' row.
    GamePayout::grant_period(
        &client,
        GamePayoutPeriodGrant {
            user_id: user.id,
            game: "nethack",
            payout_kind: "ascension",
            period_kind: "lifetime",
            period_key: "once",
            amount: 20_000,
            chip_move: ChipMove::NethackAscension,
        },
    )
    .await
    .expect("legacy claim");

    let keys = run_keys("xlogfile:900");
    let grant = GamePayout::grant_multi(&mut client, run_grant(user.id, &keys))
        .await
        .expect("first gated repeat pays");
    assert!(grant.credited);
    assert_eq!(grant.balance, 70_000);
}

#[tokio::test]
async fn concurrent_multi_grants_settle_to_one_payout() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "multi-race").await;

    // Two replicas landing two different runs at the same instant: the
    // advisory lock serializes them and the window lets exactly one through.
    let one = test_db.db.clone();
    let two = test_db.db.clone();
    let user_id = user.id;
    let left = tokio::spawn(async move {
        let mut client = one.get().await.expect("db client");
        let keys = run_keys("xlogfile:900");
        GamePayout::grant_multi(&mut client, run_grant(user_id, &keys))
            .await
            .expect("left grant")
    });
    let right = tokio::spawn(async move {
        let mut client = two.get().await.expect("db client");
        let keys = run_keys("xlogfile:1800");
        GamePayout::grant_multi(&mut client, run_grant(user_id, &keys))
            .await
            .expect("right grant")
    });
    let (left, right) = (left.await.expect("left"), right.await.expect("right"));
    assert_eq!(
        [left.credited, right.credited]
            .iter()
            .filter(|credited| **credited)
            .count(),
        1
    );

    let client = test_db.db.get().await.expect("db client");
    assert_eq!(claim_rows(&client, user.id).await, 2);
    assert_eq!(ledger_rows(&client, user.id).await, (1, 50_000));
}
