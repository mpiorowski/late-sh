use crate::{
    models::{
        chips::{CHIP_FLOOR, DRINK_PURCHASE_REASON, DRINK_PURCHASE_SOURCE_KIND, UserChips},
        drinks::{
            DRUNK_DECAY_PER_HOUR, MAX_DRUNK_POINTS, UserDrinks, WELCOME_DRINK_POINTS,
            decayed_points, drunk_label_word, drunk_level,
        },
    },
    test_utils::{create_test_user, test_db},
};
use chrono::Utc;
use uuid::Uuid;

#[test]
fn decayed_points_wears_off_linearly() {
    assert_eq!(decayed_points(600, 0), 600);
    assert_eq!(decayed_points(600, 3600), 450);
    assert_eq!(decayed_points(600, 7200), 300);
    assert_eq!(decayed_points(600, 14400), 0);
    assert_eq!(decayed_points(600, 36000), 0);
}

#[test]
fn decayed_points_handles_edge_inputs() {
    assert_eq!(decayed_points(0, 3600), 0);
    assert_eq!(decayed_points(-5, 0), 0);
    // Clock skew: a last_drink_at in the future never inflates the buzz.
    assert_eq!(decayed_points(600, -3600), 600);
}

#[test]
fn drunk_level_buckets() {
    assert_eq!(drunk_level(0), 0);
    assert_eq!(drunk_level(1), 1);
    // The welcome round lands on level 1: a glow, but no printed word yet.
    assert_eq!(drunk_level(WELCOME_DRINK_POINTS), 1);
    assert_eq!(drunk_level(299), 1);
    assert_eq!(drunk_level(300), 2);
    assert_eq!(drunk_level(999), 2);
    assert_eq!(drunk_level(1000), 3);
    assert_eq!(drunk_level(1999), 3);
    assert_eq!(drunk_level(2000), 4);
    assert_eq!(drunk_level(MAX_DRUNK_POINTS), 4);
}

#[test]
fn drunk_label_word_starts_at_level_two() {
    // Below 300 points the glow stands alone; from level 2 up a word prints.
    assert_eq!(drunk_label_word(0), None);
    assert_eq!(drunk_label_word(1), None);
    assert_eq!(drunk_label_word(drunk_level(WELCOME_DRINK_POINTS)), None);
    assert_eq!(drunk_label_word(2), Some("buzzed"));
    assert_eq!(drunk_label_word(3), Some("sloshed"));
    assert_eq!(drunk_label_word(4), Some("wasted"));
}

#[test]
fn max_cap_dries_out_within_active_window() {
    // The 36h window in all_active must cover the slowest sober-up.
    let hours_to_sober = (MAX_DRUNK_POINTS + DRUNK_DECAY_PER_HOUR - 1) / DRUNK_DECAY_PER_HOUR;
    assert!(hours_to_sober <= 36);
    assert_eq!(decayed_points(MAX_DRUNK_POINTS, hours_to_sober * 3600), 0);
}

#[test]
fn effective_points_uses_last_drink_at() {
    let now = Utc::now();
    let drinks = UserDrinks {
        user_id: Uuid::nil(),
        drunk_points: 600,
        lifetime_spent: 600,
        drink_count: 1,
        last_drink_at: now - chrono::Duration::hours(1),
    };
    assert_eq!(drinks.effective_points(now), 450);
    assert_eq!(drinks.level(now), 2);
}

#[tokio::test]
async fn record_purchase_creates_then_decays_and_accumulates() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "drinks-decay").await;

    let first = UserDrinks::record_purchase(&client, user.id, 600)
        .await
        .expect("first purchase");
    assert_eq!(first.drunk_points, 600);
    assert_eq!(first.lifetime_spent, 600);
    assert_eq!(first.drink_count, 1);

    // Backdate the last drink by one hour; the next purchase must apply
    // one hour of decay before adding (pins the SQL EPOCH cast chain).
    client
        .execute(
            "UPDATE user_drinks
             SET last_drink_at = last_drink_at - interval '1 hour'
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("backdate");

    let second = UserDrinks::record_purchase(&client, user.id, 100)
        .await
        .expect("second purchase");
    assert_eq!(second.drunk_points, 600 - DRUNK_DECAY_PER_HOUR + 100);
    assert_eq!(second.lifetime_spent, 700);
    assert_eq!(second.drink_count, 2);
}

#[tokio::test]
async fn record_purchase_caps_the_buzz() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "drinks-cap").await;

    for _ in 0..4 {
        UserDrinks::record_purchase(&client, user.id, 2_000)
            .await
            .expect("purchase");
    }
    let drinks = UserDrinks::find(&client, user.id)
        .await
        .expect("find")
        .expect("row exists");
    assert_eq!(drinks.drunk_points, MAX_DRUNK_POINTS);
    assert_eq!(drinks.lifetime_spent, 8_000);
}

#[tokio::test]
async fn deduct_for_drink_respects_the_floor_and_writes_the_ledger() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "drinks-floor").await;
    UserChips::ensure(&client, user.id).await.expect("chips"); // 1000

    // 950 would leave 50, below the floor: refused.
    let refused = UserChips::deduct_for_drink(&client, user.id, 950, "top shelf")
        .await
        .expect("attempt");
    assert!(refused.is_none());

    // 900 leaves exactly the floor: poured.
    let poured = UserChips::deduct_for_drink(&client, user.id, 900, "Segfault Sour")
        .await
        .expect("attempt")
        .expect("poured");
    assert_eq!(poured.balance, CHIP_FLOOR);

    let ledger = client
        .query_one(
            "SELECT delta, reason, source_kind, source_ref
             FROM chip_ledger
             WHERE user_id = $1 AND reason = $2",
            &[&user.id, &DRINK_PURCHASE_REASON],
        )
        .await
        .expect("ledger row");
    assert_eq!(ledger.get::<_, i64>("delta"), -900);
    assert_eq!(
        ledger.get::<_, String>("source_kind"),
        DRINK_PURCHASE_SOURCE_KIND
    );
    assert_eq!(ledger.get::<_, String>("source_ref"), "Segfault Sour");
}

#[tokio::test]
async fn drink_purchase_composes_into_one_transaction() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "drinks-tx").await;
    UserChips::ensure(&client, user.id).await.expect("chips");

    // Mirrors ChipService::buy_drink: debit + buzz upsert atomically.
    let tx = client.transaction().await.expect("transaction");
    let chips = UserChips::deduct_for_drink(&tx, user.id, 400, "Bash Old Fashioned")
        .await
        .expect("debit")
        .expect("poured");
    let drinks = UserDrinks::record_purchase(&tx, user.id, 400)
        .await
        .expect("buzz");
    tx.commit().await.expect("commit");

    assert_eq!(chips.balance, 600);
    assert_eq!(drinks.drunk_points, 400);
}
