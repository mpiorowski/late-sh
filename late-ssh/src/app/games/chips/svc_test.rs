use crate::app::games::chips::svc::ChipService;
use late_core::{
    models::chips::{ChipMove, UserChips},
    test_utils::create_test_user,
};

use crate::test_helpers::new_test_db;

#[tokio::test]
async fn transfer_chips_records_atomic_gift_ledgers() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-recipient").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let (sender_balance, recipient_balance) = chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift succeeds");

    assert_eq!(sender_balance, 500);
    assert_eq!(recipient_balance, 1_500);

    let client = test_db.db.get().await.expect("db client");
    let rows = client
        .query(
            "SELECT user_id, delta, reason
             FROM chip_ledger
             WHERE user_id IN ($1, $2)
               AND reason IN ($3, $4)
             ORDER BY delta ASC",
            &[
                &sender.id,
                &recipient.id,
                &ChipMove::GiftSent.reason(),
                &ChipMove::GiftReceived.reason(),
            ],
        )
        .await
        .expect("ledger rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<_, i64>("delta"), -500);
    assert_eq!(
        rows[0].get::<_, &str>("reason"),
        ChipMove::GiftSent.reason()
    );
    assert_eq!(rows[1].get::<_, i64>("delta"), 500);
    assert_eq!(
        rows[1].get::<_, &str>("reason"),
        ChipMove::GiftReceived.reason()
    );
}

#[tokio::test]
async fn transfer_chips_initializes_recipient_without_existing_chips_row() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-init-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-init-recipient").await;
    // Only the sender starts with a chips row; the recipient has never had one.
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let (sender_balance, recipient_balance) = chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift to fresh recipient succeeds");

    assert_eq!(sender_balance, 500);
    assert_eq!(recipient_balance, 1_500);
}

#[tokio::test]
async fn transfer_chips_insufficient_funds_leaves_balances_and_ledger_untouched() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-poor-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-poor-recipient").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let error = chips
        .transfer_chips(sender.id, recipient.id, 1_000)
        .await
        .expect_err("gift fails at floor");
    assert!(error.to_string().contains("insufficient chips"));

    let client = test_db.db.get().await.expect("db client");
    let sender_balance = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&sender.id],
        )
        .await
        .expect("sender balance")
        .get::<_, i64>("balance");
    let recipient_balance = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&recipient.id],
        )
        .await
        .expect("recipient balance")
        .get::<_, i64>("balance");
    assert_eq!(sender_balance, 1_000);
    assert_eq!(recipient_balance, 1_000);

    let ledger_count = client
        .query_one(
            "SELECT count(*)::int AS count
             FROM chip_ledger
             WHERE user_id IN ($1, $2)
               AND reason IN ($3, $4)",
            &[
                &sender.id,
                &recipient.id,
                &ChipMove::GiftSent.reason(),
                &ChipMove::GiftReceived.reason(),
            ],
        )
        .await
        .expect("ledger count")
        .get::<_, i32>("count");
    assert_eq!(ledger_count, 0);
}

#[tokio::test]
async fn transfer_chips_leaves_unrelated_users_untouched() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-scope-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-scope-recipient").await;
    let bystander = create_test_user(&test_db.db, "gift-scope-bystander").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    UserChips::ensure(&client, bystander.id)
        .await
        .expect("bystander chips");

    let chips = ChipService::new(test_db.db.clone());
    chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift succeeds");

    let balance: i64 = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&bystander.id],
        )
        .await
        .expect("bystander balance")
        .get(0);
    assert_eq!(balance, 1_000, "a transfer must not touch a third user");

    let ledger_rows: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM chip_ledger WHERE user_id = $1",
            &[&bystander.id],
        )
        .await
        .expect("bystander ledger")
        .get(0);
    assert_eq!(ledger_rows, 0);
}

#[tokio::test]
async fn welcome_pour_comps_only_the_first_drink_ever() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "welcome-pour").await;
    let chips = ChipService::new(test_db.db.clone());

    let first = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("first comp succeeds")
        .expect("first comp pours");
    assert_eq!(
        first.drunk_points,
        late_core::models::drinks::WELCOME_DRINK_POINTS
    );
    assert_eq!(first.lifetime_spent, 0);

    // A tour rerun after a mid-tour disconnect: the welcome is spent.
    let second = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("second comp succeeds");
    assert!(second.is_none());

    // The comp stayed off the tab: one drink, no lifetime spend.
    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT drunk_points, lifetime_spent, drink_count
             FROM user_drinks WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("drinks row");
    assert_eq!(
        row.get::<_, i64>("drunk_points"),
        late_core::models::drinks::WELCOME_DRINK_POINTS
    );
    assert_eq!(row.get::<_, i64>("lifetime_spent"), 0);
    assert_eq!(row.get::<_, i64>("drink_count"), 1);
}

#[tokio::test]
async fn welcome_pour_never_comps_a_prior_drinker() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "welcome-pour-veteran").await;
    let client = test_db.db.get().await.expect("db client");
    late_core::models::drinks::UserDrinks::record_purchase(&client, user.id, 200)
        .await
        .expect("paid drink");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let comp = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("comp call succeeds");
    assert!(
        comp.is_none(),
        "a prior drinker never gets the welcome pour"
    );
}
