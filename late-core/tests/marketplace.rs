use late_core::{
    models::{
        cat::CatCompanion,
        chips::UserChips,
        marketplace::{
            AQUARIUM_FISH_ITEM_KIND, AQUARIUM_MAX_FISH, AQUARIUM_SKU, CAT_COMPANION_SKU,
            CHAT_BADGE_SLOT, FishActiveStatus, MARKETPLACE_SOURCE_KIND, MarketplaceItem,
            PurchaseStatus, SHOP_PURCHASE_REASON, THEMATRIX_ULTIMATE_SKU, ULTIMATE_SPELL_KIND,
            UserPurchase, WONDERLAND_ULTIMATE_SKU, adjust_aquarium_fish_active_by_sku,
            equip_owned_item_by_sku, purchase_durable_item_by_sku, unequip_slot,
        },
        ultimate_cooldown::UltimateCastCooldown,
    },
    test_utils::{create_test_user, test_db},
};
use std::time::Duration;

const CAT_COMPANION_PRICE: i64 = 3_000;
const BASIC_BADGE_PRICE: i64 = 1_000;
const AQUARIUM_PRICE: i64 = 10_000;
const AQUARIUM_FISH_PRICE: i64 = 1_000;
const AQUARIUM_MEDIUM_FISH_PRICE: i64 = 2_500;
const AQUARIUM_BIGBERT_PRICE: i64 = 10_000;
const ULTIMATE_SPELL_PRICE: i64 = 10_000_000;

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
async fn seeded_catalog_contains_badge_shop_items() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let cat_badge = items
        .iter()
        .find(|item| item.sku == "badge_cat")
        .expect("cat badge");
    let gem_badge = items
        .iter()
        .find(|item| item.sku == "badge_gem")
        .expect("gem badge");

    assert_eq!(cat_badge.item_kind, "badge");
    assert_eq!(cat_badge.slot.as_deref(), Some(CHAT_BADGE_SLOT));
    assert_eq!(cat_badge.price_chips, BASIC_BADGE_PRICE);
    assert_eq!(cat_badge.payload["emoji"], "🐱");
    assert_eq!(cat_badge.payload["tier"], "basic");
    assert!(
        items
            .iter()
            .any(|item| item.sku == "badge_lightning" && item.payload["emoji"] == "⚡")
    );
    assert!(
        items
            .iter()
            .any(|item| item.sku == "badge_droplet" && item.payload["emoji"] == "💧")
    );
    assert!(
        items
            .iter()
            .any(|item| item.sku == "badge_snowflake" && item.payload["emoji"] == "❄️")
    );
    assert!(!items.iter().any(|item| item.sku == "badge_elements"));
    assert_eq!(gem_badge.price_chips, 5_000);
    assert_eq!(gem_badge.payload["tier"], "premium");
}

#[tokio::test]
async fn seeded_aquarium_fish_are_sorted_and_priced_by_size() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let fish = items
        .iter()
        .filter(|item| item.item_kind == AQUARIUM_FISH_ITEM_KIND)
        .collect::<Vec<_>>();
    let skus = fish
        .iter()
        .map(|item| item.sku.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        skus,
        vec![
            "aquarium_fish_mj",
            "aquarium_fish_seahorse",
            "aquarium_fish_finnegan",
            "aquarium_fish_bee",
            "aquarium_fish_boxfish",
            "aquarium_fish_tiger",
            "aquarium_fish_diamondfish",
            "aquarium_fish_bumble",
            "aquarium_fish_wingfish",
            "aquarium_fish_floata",
            "aquarium_fish_squeeb",
            "aquarium_fish_wigglewort",
            "aquarium_fish_rugbert",
            "aquarium_fish_squigs",
            "aquarium_fish_jellybean",
            "aquarium_fish_oldskool",
            "aquarium_fish_bertrand",
            "aquarium_fish_bigbert",
        ]
    );

    let seahorse = fish
        .iter()
        .find(|item| item.sku == "aquarium_fish_seahorse")
        .expect("seahorse");
    let squigs = fish
        .iter()
        .find(|item| item.sku == "aquarium_fish_squigs")
        .expect("squigs");
    let bigbert = fish
        .iter()
        .find(|item| item.sku == "aquarium_fish_bigbert")
        .expect("bigbert");

    assert_eq!(seahorse.price_chips, AQUARIUM_FISH_PRICE);
    assert_eq!(seahorse.payload["size"], "small");
    assert_eq!(squigs.price_chips, AQUARIUM_MEDIUM_FISH_PRICE);
    assert_eq!(squigs.payload["size"], "medium");
    assert_eq!(bigbert.price_chips, AQUARIUM_BIGBERT_PRICE);
    assert_eq!(bigbert.payload["size"], "large");
    assert_eq!(bigbert.payload["area"], 261);
}

#[tokio::test]
async fn aquarium_fish_are_repeatable_and_active_count_is_owned_count_bound() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-repeatable").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(
        &client,
        user.id,
        AQUARIUM_PRICE + AQUARIUM_FISH_PRICE * (AQUARIUM_MAX_FISH as i64 + 1),
    )
    .await
    .expect("fund chips");

    let aquarium = purchase_durable_item_by_sku(&mut client, user.id, AQUARIUM_SKU)
        .await
        .expect("aquarium purchase")
        .expect("aquarium item");
    let first = purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
        .await
        .expect("first fish purchase")
        .expect("seahorse item");
    let second = purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
        .await
        .expect("second fish purchase")
        .expect("seahorse item");

    assert_eq!(aquarium.status, PurchaseStatus::Purchased);
    assert_eq!(first.status, PurchaseStatus::Purchased);
    assert_eq!(second.status, PurchaseStatus::QuantityAdded);
    assert_eq!(second.item.item_kind, AQUARIUM_FISH_ITEM_KIND);
    assert_eq!(second.quantity, 2);
    assert_eq!(second.active_quantity, 0);

    let empty_decrease =
        adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_seahorse", -1)
            .await
            .expect("decrease empty active fish")
            .expect("seahorse exists");
    assert_eq!(empty_decrease.status, FishActiveStatus::AtZero);

    let increase =
        adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_seahorse", 1)
            .await
            .expect("increase active fish")
            .expect("seahorse exists");
    assert_eq!(increase.status, FishActiveStatus::Changed);
    assert_eq!(increase.active_quantity, 1);

    for _ in 0..(AQUARIUM_MAX_FISH - 2) {
        purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
            .await
            .expect("bulk fish purchase")
            .expect("seahorse item");
    }
    let above_twenty = purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
        .await
        .expect("above-twenty fish purchase")
        .expect("seahorse item");
    assert_eq!(above_twenty.status, PurchaseStatus::QuantityAdded);
    assert_eq!(above_twenty.quantity, AQUARIUM_MAX_FISH + 1);
    assert_eq!(above_twenty.active_quantity, 1);

    for _ in 1..AQUARIUM_MAX_FISH {
        let increase =
            adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_seahorse", 1)
                .await
                .expect("activate owned fish")
                .expect("seahorse exists");
        assert_eq!(increase.status, FishActiveStatus::Changed);
    }
    let full =
        adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_seahorse", 1)
            .await
            .expect("active cap")
            .expect("seahorse exists");
    assert_eq!(full.status, FishActiveStatus::TankFull);
    assert_eq!(full.active_quantity, AQUARIUM_MAX_FISH);
}

#[tokio::test]
async fn aquarium_active_adjustment_rejects_projected_total_over_cap() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-projected-cap").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(
        &client,
        user.id,
        AQUARIUM_PRICE + AQUARIUM_FISH_PRICE * AQUARIUM_MAX_FISH as i64 + AQUARIUM_FISH_PRICE * 2,
    )
    .await
    .expect("fund chips");

    purchase_durable_item_by_sku(&mut client, user.id, AQUARIUM_SKU)
        .await
        .expect("aquarium purchase")
        .expect("aquarium item");
    for _ in 0..AQUARIUM_MAX_FISH - 1 {
        purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
            .await
            .expect("seahorse purchase")
            .expect("seahorse item");
    }
    for _ in 0..2 {
        purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_tiger")
            .await
            .expect("tiger purchase")
            .expect("tiger item");
    }

    for _ in 0..AQUARIUM_MAX_FISH - 1 {
        adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_seahorse", 1)
            .await
            .expect("activate seahorse")
            .expect("seahorse exists");
    }
    let too_many =
        adjust_aquarium_fish_active_by_sku(&mut client, user.id, "aquarium_fish_tiger", 2)
            .await
            .expect("activate tiger")
            .expect("tiger exists");

    assert_eq!(too_many.status, FishActiveStatus::TankFull);
    assert_eq!(too_many.active_quantity, 0);
}

#[tokio::test]
async fn fish_purchase_requires_aquarium_and_returns_current_balance() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-required-balance").await;
    let mut client = test_db.db.get().await.expect("db client");
    let balance = UserChips::add_bonus(&client, user.id, AQUARIUM_FISH_PRICE)
        .await
        .expect("fund chips")
        .balance;

    let result = purchase_durable_item_by_sku(&mut client, user.id, "aquarium_fish_seahorse")
        .await
        .expect("fish purchase")
        .expect("seahorse item");

    assert_eq!(result.status, PurchaseStatus::RequiresAquarium);
    assert_eq!(result.balance, balance);
}

#[tokio::test]
async fn seeded_catalog_contains_ultimate_spells() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let wonderland = items
        .iter()
        .find(|item| item.sku == WONDERLAND_ULTIMATE_SKU)
        .expect("wonderland ultimate");

    assert_eq!(wonderland.item_kind, ULTIMATE_SPELL_KIND);
    assert_eq!(wonderland.name, "Wonderland");
    assert_eq!(
        wonderland.description,
        "Cast a server-wide psychedelic theme. Use /ultimate in chat to cast this spell (24h cooldown)."
    );
    assert_eq!(wonderland.price_chips, ULTIMATE_SPELL_PRICE);
    assert_eq!(wonderland.payload["ultimate"], "wonderland");
    assert!(wonderland.active);

    let matrix = items
        .iter()
        .find(|item| item.sku == THEMATRIX_ULTIMATE_SKU)
        .expect("matrix ultimate");

    assert_eq!(matrix.item_kind, ULTIMATE_SPELL_KIND);
    assert_eq!(matrix.name, "The Matrix");
    assert_eq!(
        matrix.description,
        "\"Follow the White Rabbit.\" Use /ultimate in chat to cast this spell (24h cooldown)."
    );
    assert_eq!(matrix.price_chips, ULTIMATE_SPELL_PRICE);
    assert_eq!(matrix.payload["ultimate"], "thematrix");
    assert_eq!(matrix.payload["duration_ms"], 13_000);
    assert!(matrix.active);
}

#[tokio::test]
async fn cat_companion_purchase_stamps_adoption_time() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-cat-adoption").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(&client, user.id, CAT_COMPANION_PRICE)
        .await
        .expect("fund chips");

    let cat_before = CatCompanion::ensure(&client, user.id)
        .await
        .expect("ensure pre-purchase cat row");
    assert!(cat_before.adopted_at.is_none());

    let result = purchase_durable_item_by_sku(&mut client, user.id, CAT_COMPANION_SKU)
        .await
        .expect("purchase result")
        .expect("available item");
    assert_eq!(result.status, PurchaseStatus::Purchased);

    let cat_after = CatCompanion::ensure(&client, user.id)
        .await
        .expect("load cat row");
    let adopted_at = cat_after.adopted_at.expect("adoption timestamp");
    assert_eq!(cat_after.created, cat_before.created);
    assert!(adopted_at >= cat_before.created);
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
async fn ultimate_cast_cooldown_is_tracked_per_spell() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "ultimate-cooldown").await;
    let mut client = test_db.db.get().await.expect("db client");
    let cooldown = Duration::from_secs(24 * 60 * 60);

    let first_wonderland =
        UltimateCastCooldown::try_record_cast(&mut client, user.id, "wonderland", cooldown)
            .await
            .expect("first wonderland cast");
    assert!(first_wonderland.allowed);

    let second_wonderland =
        UltimateCastCooldown::try_record_cast(&mut client, user.id, "wonderland", cooldown)
            .await
            .expect("second wonderland cast");
    assert!(!second_wonderland.allowed);
    assert!(second_wonderland.remaining.as_secs() > 23 * 60 * 60);

    let first_matrix =
        UltimateCastCooldown::try_record_cast(&mut client, user.id, "thematrix", cooldown)
            .await
            .expect("first matrix cast");
    assert!(first_matrix.allowed);

    let remaining = UltimateCastCooldown::list_remaining(&client, user.id, cooldown)
        .await
        .expect("remaining cooldowns");
    assert!(
        remaining
            .iter()
            .any(|item| item.ultimate_id == "wonderland")
    );
    assert!(remaining.iter().any(|item| item.ultimate_id == "thematrix"));
}

#[tokio::test]
async fn badge_purchase_equips_one_chat_badge_per_user() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "badge-equip").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(&client, user.id, BASIC_BADGE_PRICE * 2)
        .await
        .expect("fund chips");

    let first = purchase_durable_item_by_sku(&mut client, user.id, "badge_cat")
        .await
        .expect("first purchase")
        .expect("first badge");
    let second = purchase_durable_item_by_sku(&mut client, user.id, "badge_dog")
        .await
        .expect("second purchase")
        .expect("second badge");

    assert_eq!(first.status, PurchaseStatus::Purchased);
    assert_eq!(second.status, PurchaseStatus::Purchased);

    let equipped = client
        .query(
            "SELECT i.sku
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND p.equipped_slot = $2
             ORDER BY i.sku",
            &[&user.id, &CHAT_BADGE_SLOT],
        )
        .await
        .expect("equipped rows");
    assert_eq!(equipped.len(), 1);
    assert_eq!(equipped[0].get::<_, String>("sku"), "badge_dog");

    let equip_first = equip_owned_item_by_sku(&mut client, user.id, "badge_cat")
        .await
        .expect("equip first")
        .expect("badge cat exists");
    assert_eq!(
        equip_first.status,
        late_core::models::marketplace::EquipStatus::Equipped
    );

    let equipped = client
        .query_one(
            "SELECT i.sku
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND p.equipped_slot = $2",
            &[&user.id, &CHAT_BADGE_SLOT],
        )
        .await
        .expect("equipped row");
    assert_eq!(equipped.get::<_, String>("sku"), "badge_cat");

    let changed = unequip_slot(&mut client, user.id, CHAT_BADGE_SLOT)
        .await
        .expect("unequip badge");
    assert!(changed);

    let equipped_count = client
        .query_one(
            "SELECT count(*)::bigint AS count
             FROM user_purchases
             WHERE user_id = $1 AND equipped_slot = $2",
            &[&user.id, &CHAT_BADGE_SLOT],
        )
        .await
        .expect("equipped count")
        .get::<_, i64>("count");
    assert_eq!(equipped_count, 0);
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
