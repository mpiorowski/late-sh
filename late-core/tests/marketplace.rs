use late_core::{
    models::{
        bonsai::{BonsaiV2Tree, Tree},
        chips::UserChips,
        marketplace::{
            AQUARIUM_FISH_ITEM_KIND, AQUARIUM_MAX_FISH, AQUARIUM_SKU, BONSAI_VARIANT_SLOT,
            CHAT_BADGE_SLOT, CHAT_CONSUMABLE_ITEM_KIND, COMPANION_CONSUMABLE_ITEM_KIND,
            ConsumableUseStatus, DYNAMIC_BONSAI_SKU, FishActiveStatus, MARKETPLACE_SOURCE_KIND,
            MarketplaceItem, PET_COMPANION_SKU, PurchaseStatus, SHOP_PURCHASE_REASON,
            THEMATRIX_ULTIMATE_SKU, ULTIMATE_SPELL_KIND, UserPurchase, WONDERLAND_ULTIMATE_SKU,
            adjust_aquarium_fish_active_by_sku, aquarium_is_hungry, consume_aquarium_food_pinch,
            equip_owned_item_by_sku, purchase_durable_item_by_sku, unequip_slot,
        },
        pet::PetCompanion,
        ultimate_cooldown::UltimateCastCooldown,
        user::User,
    },
    test_utils::{create_test_user, test_db},
};
use serde_json::json;
use std::time::Duration;

const PET_COMPANION_PRICE: i64 = 3_000;
const DYNAMIC_BONSAI_PRICE: i64 = 1_000;
const BASIC_BADGE_PRICE: i64 = 1_000;
const AQUARIUM_PRICE: i64 = 10_000;
const AQUARIUM_FISH_PRICE: i64 = 1_000;
const AQUARIUM_MEDIUM_FISH_PRICE: i64 = 2_500;
const AQUARIUM_BIGBERT_PRICE: i64 = 10_000;
const ULTIMATE_SPELL_PRICE: i64 = 10_000_000;
const ROOM_SPARK_PRICE: i64 = 2_000;
const MESSAGE_ACCENT_PRICE: i64 = 500;
const AQUARIUM_FOOD_PRICE: i64 = 100;

#[tokio::test]
async fn seeded_catalog_contains_pet_companion_unlock() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let pet = items
        .iter()
        .find(|item| item.sku == PET_COMPANION_SKU)
        .expect("pet companion item");

    assert_eq!(pet.item_kind, "feature_unlock");
    assert_eq!(pet.name, "Pet Companion");
    assert_eq!(pet.price_chips, PET_COMPANION_PRICE);
    assert!(pet.active);
}

#[tokio::test]
async fn seeded_catalog_contains_dynamic_bonsai_unlock() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let bonsai = items
        .iter()
        .find(|item| item.sku == DYNAMIC_BONSAI_SKU)
        .expect("dynamic bonsai item");

    assert_eq!(bonsai.item_kind, "feature_unlock");
    assert_eq!(bonsai.slot.as_deref(), Some(BONSAI_VARIANT_SLOT));
    assert_eq!(bonsai.name, "Dynamic Bonsai");
    assert_eq!(bonsai.price_chips, DYNAMIC_BONSAI_PRICE);
    assert!(bonsai.active);
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
async fn seeded_catalog_contains_chat_and_companion_consumables() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let room_spark = items
        .iter()
        .find(|item| item.sku == "chat_room_spark")
        .expect("room spark item");
    let pet_food = items
        .iter()
        .find(|item| item.sku == "pet_food")
        .expect("pet food item");
    let aquarium_food = items
        .iter()
        .find(|item| item.sku == "aquarium_food")
        .expect("aquarium food item");

    assert_eq!(room_spark.item_kind, CHAT_CONSUMABLE_ITEM_KIND);
    assert_eq!(room_spark.price_chips, ROOM_SPARK_PRICE);
    assert_eq!(room_spark.payload["effect_kind"], "room_spark");
    assert_eq!(room_spark.payload["daily_limit"], true);
    assert_eq!(pet_food.item_kind, COMPANION_CONSUMABLE_ITEM_KIND);
    assert_eq!(pet_food.name, "Cat/Dog Food");
    assert_eq!(pet_food.price_chips, 150);
    assert_eq!(pet_food.payload["effect_kind"], "pet_food");
    assert_eq!(aquarium_food.item_kind, COMPANION_CONSUMABLE_ITEM_KIND);
    assert_eq!(aquarium_food.price_chips, AQUARIUM_FOOD_PRICE);
    assert_eq!(aquarium_food.payload["effect_kind"], "aquarium_food");
}

#[tokio::test]
async fn aquarium_food_purchase_can_be_consumed_from_inventory() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-food-use").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(&client, user.id, AQUARIUM_PRICE + AQUARIUM_FOOD_PRICE)
        .await
        .expect("fund chips");

    assert!(
        !aquarium_is_hungry(&client, user.id)
            .await
            .expect("hunger without aquarium")
    );

    purchase_durable_item_by_sku(&mut client, user.id, AQUARIUM_SKU)
        .await
        .expect("purchase aquarium")
        .expect("aquarium item");
    assert!(
        aquarium_is_hungry(&client, user.id)
            .await
            .expect("fresh aquarium hunger")
    );

    client
        .execute(
            "INSERT INTO user_aquarium_care (user_id, last_fed)
             VALUES ($1, current_timestamp - interval '25 hours')
             ON CONFLICT (user_id) DO UPDATE
             SET last_fed = EXCLUDED.last_fed,
                 updated = current_timestamp",
            &[&user.id],
        )
        .await
        .expect("age aquarium feed");
    assert!(
        aquarium_is_hungry(&client, user.id)
            .await
            .expect("aged aquarium hunger")
    );

    let out_of_stock = consume_aquarium_food_pinch(&mut client, user.id)
        .await
        .expect("consume before purchase");
    assert_eq!(out_of_stock.status, ConsumableUseStatus::OutOfStock);

    let purchase = purchase_durable_item_by_sku(&mut client, user.id, "aquarium_food")
        .await
        .expect("purchase food")
        .expect("aquarium food item");
    assert_eq!(purchase.status, PurchaseStatus::Purchased);
    assert_eq!(purchase.quantity, 1);

    let used = consume_aquarium_food_pinch(&mut client, user.id)
        .await
        .expect("consume food");
    assert_eq!(used.status, ConsumableUseStatus::Used);
    assert_eq!(used.quantity_remaining, 0);
    assert!(
        !aquarium_is_hungry(&client, user.id)
            .await
            .expect("fed aquarium hunger")
    );

    let empty = consume_aquarium_food_pinch(&mut client, user.id)
        .await
        .expect("consume after empty");
    assert_eq!(empty.status, ConsumableUseStatus::OutOfStock);
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
            "aquarium_fish_anchovy",
            "aquarium_fish_clownfish",
            "aquarium_fish_pufferfish",
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
async fn consumable_purchase_repeats_and_daily_limit_is_enforced() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-consumable-repeat").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(
        &client,
        user.id,
        ROOM_SPARK_PRICE + MESSAGE_ACCENT_PRICE * 2,
    )
    .await
    .expect("fund chips");

    let first_spark = purchase_durable_item_by_sku(&mut client, user.id, "chat_room_spark")
        .await
        .expect("first spark")
        .expect("spark item");
    let second_spark = purchase_durable_item_by_sku(&mut client, user.id, "chat_room_spark")
        .await
        .expect("second spark")
        .expect("spark item");
    let first_accent = purchase_durable_item_by_sku(&mut client, user.id, "chat_message_accent")
        .await
        .expect("first accent")
        .expect("accent item");
    let second_accent = purchase_durable_item_by_sku(&mut client, user.id, "chat_message_accent")
        .await
        .expect("second accent")
        .expect("accent item");

    assert_eq!(first_spark.status, PurchaseStatus::Purchased);
    assert_eq!(second_spark.status, PurchaseStatus::DailyLimitReached);
    assert_eq!(first_accent.status, PurchaseStatus::Purchased);
    assert_eq!(second_accent.status, PurchaseStatus::QuantityAdded);
    assert_eq!(second_accent.quantity, 2);
}

#[tokio::test]
async fn pet_companion_purchase_stamps_adoption_time() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-pet-adoption").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(&client, user.id, PET_COMPANION_PRICE)
        .await
        .expect("fund chips");

    let pet_before = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure pre-purchase pet row");
    assert!(pet_before.adopted_at.is_none());

    let result = purchase_durable_item_by_sku(&mut client, user.id, PET_COMPANION_SKU)
        .await
        .expect("purchase result")
        .expect("available item");
    assert_eq!(result.status, PurchaseStatus::Purchased);

    let pet_after = PetCompanion::ensure(&client, user.id)
        .await
        .expect("load pet row");
    let adopted_at = pet_after.adopted_at.expect("adoption timestamp");
    assert_eq!(pet_after.created, pet_before.created);
    assert!(adopted_at >= pet_before.created);
}

#[tokio::test]
async fn durable_purchase_debits_chips_and_records_entitlement() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-purchase").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::add_bonus(&client, user.id, PET_COMPANION_PRICE)
        .await
        .expect("fund chips")
        .balance;

    let result = purchase_durable_item_by_sku(&mut client, user.id, PET_COMPANION_SKU)
        .await
        .expect("purchase result")
        .expect("available item");

    assert_eq!(result.status, PurchaseStatus::Purchased);
    assert_eq!(result.balance, starting_balance - PET_COMPANION_PRICE);

    let chips = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    assert_eq!(chips.balance, starting_balance - PET_COMPANION_PRICE);

    let purchases = UserPurchase::list_for_user(&client, user.id)
        .await
        .expect("purchases");
    assert_eq!(purchases.len(), 1);
    assert_eq!(purchases[0].item_id, result.item.id);
    assert_eq!(purchases[0].quantity, 1);
    assert_eq!(purchases[0].purchased_price_chips, PET_COMPANION_PRICE);

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
    assert_eq!(row.get::<_, i64>("delta"), -PET_COMPANION_PRICE);
    assert_eq!(row.get::<_, String>("reason"), SHOP_PURCHASE_REASON);
    assert_eq!(
        row.get::<_, Option<String>>("source_kind"),
        Some(MARKETPLACE_SOURCE_KIND.to_string())
    );
    assert_eq!(
        row.get::<_, Option<String>>("source_ref"),
        Some(PET_COMPANION_SKU.to_string())
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
async fn dynamic_bonsai_purchase_equips_bonsai_variant_slot() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "dynamic-bonsai-equip").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::add_bonus(&client, user.id, DYNAMIC_BONSAI_PRICE)
        .await
        .expect("fund chips");

    let purchase = purchase_durable_item_by_sku(&mut client, user.id, DYNAMIC_BONSAI_SKU)
        .await
        .expect("purchase dynamic bonsai")
        .expect("dynamic bonsai exists");
    assert_eq!(purchase.status, PurchaseStatus::Purchased);

    let equipped = client
        .query_one(
            "SELECT i.sku
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND p.equipped_slot = $2",
            &[&user.id, &BONSAI_VARIANT_SLOT],
        )
        .await
        .expect("equipped bonsai row");
    assert_eq!(equipped.get::<_, String>("sku"), DYNAMIC_BONSAI_SKU);

    let changed = unequip_slot(&mut client, user.id, BONSAI_VARIANT_SLOT)
        .await
        .expect("unequip dynamic bonsai");
    assert!(changed);
}

#[tokio::test]
async fn chat_author_metadata_marks_dynamic_bonsai_only_when_selected() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "dynamic-bonsai-chat-badge").await;
    let mut client = test_db.db.get().await.expect("db client");
    Tree::ensure(&client, user.id, 7)
        .await
        .expect("classic bonsai");
    BonsaiV2Tree::ensure(
        &client,
        user.id,
        7,
        chrono::Utc::now().date_naive(),
        json!({"version": 1, "next_id": 1, "branches": []}),
        "DYN",
    )
    .await
    .expect("dynamic bonsai");

    let metadata = User::list_chat_author_metadata(&client, &[user.id])
        .await
        .expect("metadata before purchase");
    assert!(!metadata[0].dynamic_bonsai_selected);
    assert_eq!(metadata[0].bonsai_v2_badge_glyph.as_deref(), Some("DYN"));

    UserChips::add_bonus(&client, user.id, DYNAMIC_BONSAI_PRICE)
        .await
        .expect("fund chips");
    purchase_durable_item_by_sku(&mut client, user.id, DYNAMIC_BONSAI_SKU)
        .await
        .expect("purchase dynamic bonsai")
        .expect("dynamic bonsai exists");

    let metadata = User::list_chat_author_metadata(&client, &[user.id])
        .await
        .expect("metadata after purchase");
    assert!(metadata[0].dynamic_bonsai_selected);

    unequip_slot(&mut client, user.id, BONSAI_VARIANT_SLOT)
        .await
        .expect("unequip dynamic bonsai");
    let metadata = User::list_chat_author_metadata(&client, &[user.id])
        .await
        .expect("metadata after unequip");
    assert!(!metadata[0].dynamic_bonsai_selected);
}

#[tokio::test]
async fn durable_purchase_is_idempotent_for_owned_item() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-idempotent").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::add_bonus(&client, user.id, PET_COMPANION_PRICE)
        .await
        .expect("fund chips")
        .balance;

    let first = purchase_durable_item_by_sku(&mut client, user.id, PET_COMPANION_SKU)
        .await
        .expect("first purchase")
        .expect("available item");
    let second = purchase_durable_item_by_sku(&mut client, user.id, PET_COMPANION_SKU)
        .await
        .expect("second purchase")
        .expect("available item");

    assert_eq!(first.status, PurchaseStatus::Purchased);
    assert_eq!(second.status, PurchaseStatus::AlreadyOwned);
    assert_eq!(second.balance, starting_balance - PET_COMPANION_PRICE);

    let chips = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    assert_eq!(chips.balance, starting_balance - PET_COMPANION_PRICE);

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
