use late_core::{
    models::{
        chips::UserChips,
        marketplace::{
            CAT_COMPANION_SKU, MARKETPLACE_SOURCE_KIND, MarketplaceItem, PurchaseStatus,
            SHOP_PURCHASE_REASON, UserPurchase, purchase_durable_item_by_sku,
        },
    },
    test_utils::{create_test_user, test_db},
};

const CAT_COMPANION_PRICE: i64 = 3_000;

#[tokio::test]
async fn seeded_catalog_contains_cat_companion_unlock() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let cat = items
        .iter()
        .find(|item| item.sku == CAT_COMPANION_SKU)
        .expect("cat companion item");

    assert_eq!(cat.item_kind, "feature_unlock");
    assert_eq!(cat.name, "Cat Companion");
    assert_eq!(cat.price_chips, CAT_COMPANION_PRICE);
    assert!(cat.active);
}

#[tokio::test]
async fn durable_purchase_debits_chips_and_records_entitlement() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-purchase").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::add_bonus(&client, user.id, CAT_COMPANION_PRICE)
        .await
        .expect("fund chips")
        .balance;

    let result = purchase_durable_item_by_sku(&mut client, user.id, CAT_COMPANION_SKU)
        .await
        .expect("purchase result")
        .expect("available item");

    assert_eq!(result.status, PurchaseStatus::Purchased);
    assert_eq!(result.balance, starting_balance - CAT_COMPANION_PRICE);

    let chips = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    assert_eq!(chips.balance, starting_balance - CAT_COMPANION_PRICE);

    let purchases = UserPurchase::list_for_user(&client, user.id)
        .await
        .expect("purchases");
    assert_eq!(purchases.len(), 1);
    assert_eq!(purchases[0].item_id, result.item.id);
    assert_eq!(purchases[0].quantity, 1);
    assert_eq!(purchases[0].purchased_price_chips, CAT_COMPANION_PRICE);

    let row = client
        .query_one(
            "SELECT delta, reason, source_kind, source_ref
             FROM chip_ledger
             WHERE user_id = $1
               AND reason = $2
             ORDER BY created_at DESC
             LIMIT 1",
            &[&user.id, &SHOP_PURCHASE_REASON],
        )
        .await
        .expect("ledger row");
    assert_eq!(row.get::<_, i64>("delta"), -CAT_COMPANION_PRICE);
    assert_eq!(row.get::<_, String>("reason"), SHOP_PURCHASE_REASON);
    assert_eq!(
        row.get::<_, Option<String>>("source_kind"),
        Some(MARKETPLACE_SOURCE_KIND.to_string())
    );
    assert_eq!(
        row.get::<_, Option<String>>("source_ref"),
        Some(CAT_COMPANION_SKU.to_string())
    );
}

#[tokio::test]
async fn durable_purchase_is_idempotent_for_owned_item() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-idempotent").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::add_bonus(&client, user.id, CAT_COMPANION_PRICE)
        .await
        .expect("fund chips")
        .balance;

    let first = purchase_durable_item_by_sku(&mut client, user.id, CAT_COMPANION_SKU)
        .await
        .expect("first purchase")
        .expect("available item");
    let second = purchase_durable_item_by_sku(&mut client, user.id, CAT_COMPANION_SKU)
        .await
        .expect("second purchase")
        .expect("available item");

    assert_eq!(first.status, PurchaseStatus::Purchased);
    assert_eq!(second.status, PurchaseStatus::AlreadyOwned);
    assert_eq!(second.balance, starting_balance - CAT_COMPANION_PRICE);

    let chips = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    assert_eq!(chips.balance, starting_balance - CAT_COMPANION_PRICE);

    let purchase_count = client
        .query_one(
            "SELECT count(*)::bigint AS count
             FROM user_purchases
             WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("purchase count")
        .get::<_, i64>("count");
    assert_eq!(purchase_count, 1);

    let debit_count = client
        .query_one(
            "SELECT count(*)::bigint AS count
             FROM chip_ledger
             WHERE user_id = $1 AND reason = $2",
            &[&user.id, &SHOP_PURCHASE_REASON],
        )
        .await
        .expect("ledger count")
        .get::<_, i64>("count");
    assert_eq!(debit_count, 1);
}
