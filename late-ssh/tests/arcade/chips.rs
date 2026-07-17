use late_core::{
    models::{
        chips::{
            CHIP_GIFT_RECEIVED_REASON, CHIP_GIFT_SENT_REASON, DRINK_PURCHASE_REASON, UserChips,
        },
        drinks::UserDrinks,
    },
    test_utils::create_test_user,
};
use late_ssh::app::games::chips::svc::ChipService;

use super::helpers::new_test_db;

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
                &CHIP_GIFT_SENT_REASON,
                &CHIP_GIFT_RECEIVED_REASON,
            ],
        )
        .await
        .expect("ledger rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<_, i64>("delta"), -500);
    assert_eq!(rows[0].get::<_, &str>("reason"), CHIP_GIFT_SENT_REASON);
    assert_eq!(rows[1].get::<_, i64>("delta"), 500);
    assert_eq!(rows[1].get::<_, &str>("reason"), CHIP_GIFT_RECEIVED_REASON);
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
                &CHIP_GIFT_SENT_REASON,
                &CHIP_GIFT_RECEIVED_REASON,
            ],
        )
        .await
        .expect("ledger count")
        .get::<_, i32>("count");
    assert_eq!(ledger_count, 0);
}

#[tokio::test]
async fn buy_drink_for_charges_payer_and_buzzes_recipient() {
    let test_db = new_test_db().await;
    let payer = create_test_user(&test_db.db, "drink-gift-payer").await;
    let recipient = create_test_user(&test_db.db, "drink-gift-recipient").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, payer.id)
        .await
        .expect("payer chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let purchase = chips
        .buy_drink_for(payer.id, recipient.id, 300, "Kernel Panic Punch")
        .await
        .expect("gift drink succeeds")
        .expect("poured");
    assert_eq!(purchase.balance, 700);
    assert_eq!(purchase.drunk_points, 300);

    let client = test_db.db.get().await.expect("db client");
    let payer_balance = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&payer.id],
        )
        .await
        .expect("payer balance")
        .get::<_, i64>("balance");
    assert_eq!(payer_balance, 700);

    let recipient_drinks = UserDrinks::find(&client, recipient.id)
        .await
        .expect("recipient drinks")
        .expect("recipient got buzz");
    assert_eq!(recipient_drinks.drunk_points, 300);
    assert_eq!(recipient_drinks.lifetime_spent, 300);

    let ledger = client
        .query_one(
            "SELECT user_id, delta, reason, source_ref
             FROM chip_ledger
             WHERE user_id = $1 AND reason = $2",
            &[&payer.id, &DRINK_PURCHASE_REASON],
        )
        .await
        .expect("ledger row");
    assert_eq!(ledger.get::<_, uuid::Uuid>("user_id"), payer.id);
    assert_eq!(ledger.get::<_, i64>("delta"), -300);
    assert_eq!(ledger.get::<_, String>("source_ref"), "Kernel Panic Punch");
}
