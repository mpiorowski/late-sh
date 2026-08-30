use crate::{
    models::{
        bonsai::{BonsaiV2Tree, Tree},
        chips::{ChipMove, UserChips},
        marketplace::{
            AQUARIUM_FISH_ITEM_KIND, AQUARIUM_MAX_FISH, AQUARIUM_SKU, BONSAI_CONSUMABLE_ITEM_KIND,
            BONSAI_DECAY_PROTECTION_KIND, BONSAI_DECAY_SHIELD_SKU, BONSAI_VARIANT_SLOT,
            CHAT_BADGE_SLOT, CHAT_CONSUMABLE_ITEM_KIND, CHAT_FLAG_SLOT,
            COMPANION_CONSUMABLE_ITEM_KIND, ConsumableUseStatus, DYNAMIC_BONSAI_SKU,
            FishActiveStatus, MarketplaceItem, PET_COMPANION_SKU, PurchaseStatus,
            THEMATRIX_ULTIMATE_SKU, ULTIMATE_SPELL_KIND, USERNAME_EFFECT_ITEM_KIND, UserPurchase,
            WONDERLAND_ULTIMATE_SKU, adjust_aquarium_fish_active_by_sku, aquarium_is_hungry,
            consume_aquarium_food_pinch, equip_owned_item_by_sku, purchase_durable_item_by_sku,
            purchase_item_by_sku_with_chat_effect, purchase_item_by_sku_with_custom_title,
            purchase_item_by_sku_with_username_effect, rental_duration_secs, unequip_slot,
        },
        pet::PetCompanion,
        rental::{
            BADGE_RENTAL_ITEM_KIND, BadgeRental, CustomTitle, RENTAL_DAY_SECS, RENTAL_MONTH_SECS,
            TITLE_EFFECT_KIND, TITLE_MAX_LEN, TITLE_RENTAL_ITEM_KIND, is_custom_title,
            title_from_payload,
        },
        shop_consumable_effect::ShopConsumableEffect,
        ultimate_cooldown::UltimateCastCooldown,
        user::User,
        username_effect::{
            GlowColor, GradientPair, USERNAME_EFFECT_KIND, USERNAME_GLOW_MONTH_SKU,
            USERNAME_GLOW_SKU, USERNAME_GRADIENT_MONTH_SKU, USERNAME_GRADIENT_SKU,
            USERNAME_SHIMMER_MONTH_SKU, USERNAME_SHIMMER_SKU, UsernameEffect,
        },
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
/// Lowered from 10,000,000 by migration 157: at ten million neither spell
/// ever sold, and the burn milestones ladder up to half of the new ceiling.
const ULTIMATE_SPELL_PRICE: i64 = 1_000_000;
const ROOM_SPARK_PRICE: i64 = 2_000;
const AQUARIUM_FOOD_PRICE: i64 = 100;
const BADGE_RENTAL_DAY_PRICE: i64 = 100;
const BADGE_RENTAL_MONTH_PRICE: i64 = 3_000;
const CUSTOM_TITLE_DAY_PRICE: i64 = 1_000;
const CUSTOM_TITLE_MONTH_PRICE: i64 = 40_000;

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
async fn seeded_catalog_rents_every_badge_and_flag_and_retires_the_permanent_skus() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");

    // sku, emoji, slot, day price, month price
    let expectations = [
        ("badge_cat", "🐱", CHAT_BADGE_SLOT, 100, 4_000),
        ("badge_lightning", "⚡", CHAT_BADGE_SLOT, 100, 4_000),
        ("badge_gem", "💎", CHAT_BADGE_SLOT, 250, 10_000),
        ("badge_flag_pl", "🇵🇱", CHAT_FLAG_SLOT, 100, 4_000),
    ];
    for (legacy_sku, emoji, slot, day_price, month_price) in expectations {
        let day = items
            .iter()
            .find(|item| item.sku == format!("{legacy_sku}_day"))
            .unwrap_or_else(|| panic!("missing {legacy_sku}_day"));
        let month = items
            .iter()
            .find(|item| item.sku == format!("{legacy_sku}_month"))
            .unwrap_or_else(|| panic!("missing {legacy_sku}_month"));

        for (item, price, duration) in [
            (day, day_price, RENTAL_DAY_SECS),
            (month, month_price, RENTAL_MONTH_SECS),
        ] {
            assert_eq!(item.item_kind, BADGE_RENTAL_ITEM_KIND);
            // A rental never equips anything: the slot it fills lives in the
            // payload, and reaches chat through an effect row.
            assert_eq!(item.slot, None);
            assert_eq!(item.price_chips, price);
            assert_eq!(item.payload["emoji"], emoji);
            assert_eq!(item.payload["slot"], slot);
            assert_eq!(rental_duration_secs(item), duration);
            assert_eq!(
                BadgeRental::from_payload(&item.payload)
                    .expect("renderable rental")
                    .emoji,
                emoji
            );
        }
        // The month tier lists directly under its day twin.
        assert_eq!(month.sort_order, day.sort_order + 5);

        // The permanent SKU is retired, not deleted: history in
        // `user_purchases` still resolves, and legacy owners keep rendering.
        assert!(
            !items.iter().any(|item| item.sku == legacy_sku),
            "{legacy_sku} must not be buyable any more"
        );
        let legacy = client
            .query_one(
                "SELECT active, item_kind FROM marketplace_items WHERE sku = $1",
                &[&legacy_sku],
            )
            .await
            .expect("legacy row still present");
        assert!(!legacy.get::<_, bool>("active"));
        assert_eq!(legacy.get::<_, String>("item_kind"), "badge");
    }

    // Nothing permanent is left on either chat-label slot.
    assert!(
        !items
            .iter()
            .any(|item| item.slot.as_deref() == Some(CHAT_BADGE_SLOT)
                || item.slot.as_deref() == Some(CHAT_FLAG_SLOT))
    );
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
async fn hack_room_is_retired_and_room_bump_leads_the_chat_consumables() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    // Catalog order is the Chat tab's order: Room Bump first, nothing named
    // Hack Room anywhere on sale.
    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let chat_consumables: Vec<&str> = items
        .iter()
        .filter(|item| item.item_kind == CHAT_CONSUMABLE_ITEM_KIND)
        .map(|item| item.sku.as_str())
        .collect();
    assert_eq!(
        chat_consumables,
        vec![
            "chat_room_bump",
            "chat_room_spark",
            "chat_room_glow",
            "chat_room_pulse"
        ]
    );

    // Retired, not deleted: the row stays for purchase history, inactive.
    let active: bool = client
        .query_one(
            "SELECT active FROM marketplace_items WHERE sku = 'chat_pinned_vibe'",
            &[],
        )
        .await
        .expect("hack room row")
        .get(0);
    assert!(!active);
}

#[tokio::test]
async fn companion_shop_items_are_ordered_by_care_flow() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let companion_skus = items
        .iter()
        .filter(|item| {
            matches!(
                item.sku.as_str(),
                DYNAMIC_BONSAI_SKU
                    | PET_COMPANION_SKU
                    | "pet_food"
                    | AQUARIUM_SKU
                    | "aquarium_food"
            )
        })
        .map(|item| item.sku.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        companion_skus,
        vec![
            DYNAMIC_BONSAI_SKU,
            PET_COMPANION_SKU,
            "pet_food",
            AQUARIUM_SKU,
            "aquarium_food",
        ]
    );
}

#[tokio::test]
async fn aquarium_food_purchase_can_be_consumed_from_inventory() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "aquarium-food-use").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        AQUARIUM_PRICE + AQUARIUM_FOOD_PRICE,
        None,
    )
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
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        AQUARIUM_PRICE + AQUARIUM_FISH_PRICE * (AQUARIUM_MAX_FISH as i64 + 1),
        None,
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
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        AQUARIUM_PRICE + AQUARIUM_FISH_PRICE * AQUARIUM_MAX_FISH as i64 + AQUARIUM_FISH_PRICE * 2,
        None,
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
    let balance = UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        AQUARIUM_FISH_PRICE,
        None,
    )
    .await
    .expect("fund chips")
    .expect("credited")
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
    UserChips::apply(&**client, user.id, ChipMove::Credit, ROOM_SPARK_PRICE, None)
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
    assert_eq!(first_spark.status, PurchaseStatus::Purchased);
    assert_eq!(second_spark.status, PurchaseStatus::DailyLimitReached);
}

#[tokio::test]
async fn pet_companion_purchase_stamps_adoption_time() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "marketplace-pet-adoption").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        PET_COMPANION_PRICE,
        None,
    )
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
    let starting_balance = UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        PET_COMPANION_PRICE,
        None,
    )
    .await
    .expect("fund chips")
    .expect("credited")
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
            &[&user.id, &ChipMove::ShopPurchase.reason()],
        )
        .await
        .expect("ledger row");
    assert_eq!(row.get::<_, i64>("delta"), -PET_COMPANION_PRICE);
    assert_eq!(
        row.get::<_, String>("reason"),
        ChipMove::ShopPurchase.reason()
    );
    assert_eq!(
        row.get::<_, Option<String>>("source_kind"),
        Some(ChipMove::ShopPurchase.source_kind().to_string())
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
async fn dynamic_bonsai_purchase_equips_bonsai_variant_slot() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "dynamic-bonsai-equip").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        DYNAMIC_BONSAI_PRICE,
        None,
    )
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

    // Going back to dynamic re-equips what is already owned, without buying
    // again. `bonsai_variant` is the only slot anything still equips, so this
    // is the only coverage `equip_owned_item_by_sku` has.
    let requipped = equip_owned_item_by_sku(&mut client, user.id, DYNAMIC_BONSAI_SKU)
        .await
        .expect("re-equip dynamic bonsai")
        .expect("dynamic bonsai exists");
    assert_eq!(
        requipped.status,
        crate::models::marketplace::EquipStatus::Equipped
    );
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

    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        DYNAMIC_BONSAI_PRICE,
        None,
    )
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
    let starting_balance = UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        PET_COMPANION_PRICE,
        None,
    )
    .await
    .expect("fund chips")
    .expect("credited")
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
            &[&user.id, &ChipMove::ShopPurchase.reason()],
        )
        .await
        .expect("ledger count")
        .get::<_, i64>("count");
    assert_eq!(debit_count, 1);
}

/// Buys a permanent badge the way its owner did before rentals retired the
/// SKU. Migration 148 leaves the row in place with `active = false`, so this
/// flips it on for the purchase and back off again: the user ends up in
/// exactly the state a pre-rental owner is in today, reached through the same
/// purchase path production used.
async fn buy_retired_permanent_badge(
    client: &mut tokio_postgres::Client,
    user_id: uuid::Uuid,
    sku: &str,
) -> crate::models::marketplace::PurchaseResult {
    client
        .execute(
            "UPDATE marketplace_items SET active = true WHERE sku = $1",
            &[&sku],
        )
        .await
        .expect("un-retire legacy badge");
    let result = purchase_durable_item_by_sku(client, user_id, sku)
        .await
        .expect("legacy badge purchase")
        .expect("legacy badge exists");
    client
        .execute(
            "UPDATE marketplace_items SET active = false WHERE sku = $1",
            &[&sku],
        )
        .await
        .expect("re-retire legacy badge");
    result
}

/// Every live user-scoped effect row of one kind for one user.
async fn active_effect_rows(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
    effect_kind: &str,
) -> Vec<ShopConsumableEffect> {
    ShopConsumableEffect::active_user_effects_for_user(client, user_id, &[effect_kind])
        .await
        .expect("active effects")
}

/// Ages a live effect row out, the way its `ends_at` would pass on its own.
async fn expire_effect_rows(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
    effect_kind: &str,
) {
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET ends_at = current_timestamp - INTERVAL '1 second'
             WHERE user_id = $1 AND effect_kind = $2 AND room_id IS NULL",
            &[&user_id, &effect_kind],
        )
        .await
        .expect("expire effect rows");
}

async fn chat_label(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
) -> (Option<String>, Option<String>) {
    let metadata = User::list_chat_author_metadata(client, &[user_id])
        .await
        .expect("chat author metadata");
    let row = metadata.into_iter().next().expect("one row per user");
    (row.chat_badge, row.chat_flag)
}

#[tokio::test]
async fn badge_rental_activates_one_row_per_slot_and_a_rebuy_replaces_it() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "badge-rental-buy").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BADGE_RENTAL_MONTH_PRICE * 2,
        None,
    )
    .await
    .expect("fund chips")
    .expect("credited")
    .balance;

    let before = chrono::Utc::now();
    let result = purchase_item_by_sku_with_chat_effect(&mut client, user.id, "badge_cat_day", None)
        .await
        .expect("rent cat badge");
    let purchase = result.purchase.expect("item available");
    assert_eq!(purchase.status, PurchaseStatus::Purchased);
    assert_eq!(purchase.balance, starting_balance - BADGE_RENTAL_DAY_PRICE);

    let row = result.badge_rental.expect("activated rental row");
    assert_eq!(row.user_id, user.id);
    assert_eq!(row.room_id, None);
    assert_eq!(row.effect_kind, CHAT_BADGE_SLOT);
    assert_eq!(row.source_sku, "badge_cat_day");
    assert_eq!(row.payload["emoji"], "🐱");
    let expected_end = before + chrono::Duration::seconds(RENTAL_DAY_SECS);
    assert!(row.ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(row.ends_at <= expected_end + chrono::Duration::seconds(60));
    assert_eq!(
        chat_label(&client, user.id).await,
        (Some("🐱".into()), None)
    );

    // A rebuy across badges and tiers replaces the live row and resets the
    // clock: still exactly one badge.
    let before = chrono::Utc::now();
    let row = purchase_item_by_sku_with_chat_effect(&mut client, user.id, "badge_dog_month", None)
        .await
        .expect("rent dog badge")
        .badge_rental
        .expect("activated rental row");
    assert_eq!(row.source_sku, "badge_dog_month");
    let expected_end = before + chrono::Duration::seconds(RENTAL_MONTH_SECS);
    assert!(row.ends_at >= expected_end - chrono::Duration::seconds(60));
    let rows = active_effect_rows(&client, user.id, CHAT_BADGE_SLOT).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, row.id);
    assert_eq!(
        chat_label(&client, user.id).await,
        (Some("🐶".into()), None)
    );

    // The flag is its own slot: renting one leaves the badge alone.
    purchase_item_by_sku_with_chat_effect(&mut client, user.id, "badge_flag_pl_day", None)
        .await
        .expect("rent flag");
    assert_eq!(
        active_effect_rows(&client, user.id, CHAT_BADGE_SLOT)
            .await
            .len(),
        1
    );
    assert_eq!(
        active_effect_rows(&client, user.id, CHAT_FLAG_SLOT)
            .await
            .len(),
        1
    );
    assert_eq!(
        chat_label(&client, user.id).await,
        (Some("🐶".into()), Some("🇵🇱".into()))
    );

    // Expiry needs no background task: the label query stops seeing the row
    // the moment `ends_at` passes.
    expire_effect_rows(&client, user.id, CHAT_BADGE_SLOT).await;
    expire_effect_rows(&client, user.id, CHAT_FLAG_SLOT).await;
    assert_eq!(chat_label(&client, user.id).await, (None, None));
}

#[tokio::test]
async fn a_permanent_badge_equip_never_reaches_the_chat_label() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "badge-rental-legacy").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BASIC_BADGE_PRICE + BADGE_RENTAL_DAY_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    // The state migration 165 cleared: a permanent badge, bought before
    // rentals existed, still sitting in `equipped_slot`. Nothing can create
    // this any more, and the label query no longer reads it, so the only way
    // to wear a badge is to rent one.
    buy_retired_permanent_badge(&mut client, user.id, "badge_cat").await;
    assert_eq!(chat_label(&client, user.id).await, (None, None));

    purchase_item_by_sku_with_chat_effect(&mut client, user.id, "badge_dog_day", None)
        .await
        .expect("rent dog badge");
    assert_eq!(
        chat_label(&client, user.id).await,
        (Some("🐶".into()), None)
    );

    // The rental lapsing leaves the label bare. Before 165 the permanent
    // badge came back here, which is what made a rented badge a mask rather
    // than the whole thing.
    expire_effect_rows(&client, user.id, CHAT_BADGE_SLOT).await;
    assert_eq!(chat_label(&client, user.id).await, (None, None));
}

/// Migration 165's end state, asserted against the migrated database rather
/// than the migration text: nothing anywhere still equips a chat badge or a
/// flag, and no catalog row could put one there again.
#[tokio::test]
async fn no_purchase_equips_a_chat_badge_or_flag_slot() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let equipped = client
        .query_one(
            "SELECT count(*)::bigint AS count
             FROM user_purchases
             WHERE equipped_slot IN ($1, $2)",
            &[&CHAT_BADGE_SLOT, &CHAT_FLAG_SLOT],
        )
        .await
        .expect("equipped count")
        .get::<_, i64>("count");
    assert_eq!(equipped, 0);

    let sellable = client
        .query_one(
            "SELECT count(*)::bigint AS count
             FROM marketplace_items
             WHERE active = true AND slot IN ($1, $2)",
            &[&CHAT_BADGE_SLOT, &CHAT_FLAG_SLOT],
        )
        .await
        .expect("sellable count")
        .get::<_, i64>("count");
    assert_eq!(sellable, 0);
}

#[tokio::test]
async fn a_badge_rental_never_shows_on_another_users_label() {
    let test_db = test_db().await;
    let renter = create_test_user(&test_db.db, "badge-rental-renter").await;
    let bystander = create_test_user(&test_db.db, "badge-rental-bystander").await;
    let mut client = test_db.db.get().await.expect("db client");

    purchase_item_by_sku_with_chat_effect(&mut client, renter.id, "badge_cat_day", None)
        .await
        .expect("rent cat badge");
    purchase_item_by_sku_with_chat_effect(&mut client, renter.id, "badge_flag_pl_day", None)
        .await
        .expect("rent flag");

    assert_eq!(
        chat_label(&client, renter.id).await,
        (Some("🐱".into()), Some("🇵🇱".into()))
    );
    assert_eq!(chat_label(&client, bystander.id).await, (None, None));
    assert!(
        active_effect_rows(&client, bystander.id, CHAT_BADGE_SLOT)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn curated_titles_are_retired_and_cannot_be_bought() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "curated-title-retired").await;
    let mut client = test_db.db.get().await.expect("db client");

    // The only title on sale is the one the buyer writes: nothing visible
    // carries a text of its own.
    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    assert!(
        items
            .iter()
            .filter(|item| item.item_kind == TITLE_RENTAL_ITEM_KIND)
            .all(|item| is_custom_title(&item.payload)),
        "a curated title is still on sale"
    );

    // Retired, not deleted: the 36 curated titles keep their day and month
    // rows for purchase history, switched off.
    let retired: i64 = client
        .query_one(
            "SELECT COUNT(*)
             FROM marketplace_items
             WHERE item_kind = $1
               AND active = false
               AND COALESCE((payload->>'custom')::boolean, false) = false",
            &[&TITLE_RENTAL_ITEM_KIND],
        )
        .await
        .expect("count retired titles")
        .get(0);
    assert_eq!(retired, 72);

    // A retired SKU is not for sale, funded or not: the purchase is a no-op
    // (nothing bought, nothing activated), the same contract the retired
    // permanent badges follow.
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        CUSTOM_TITLE_MONTH_PRICE,
        None,
    )
    .await
    .expect("fund chips");
    let funded = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;
    let result = purchase_item_by_sku_with_chat_effect(
        &mut client,
        user.id,
        "title_the_insufferable_day",
        None,
    )
    .await
    .expect("retired sku purchase");
    assert!(result.purchase.is_none());
    assert!(result.title_rental.is_none());
    assert!(
        active_effect_rows(&client, user.id, TITLE_EFFECT_KIND)
            .await
            .is_empty()
    );
    let balance = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;
    assert_eq!(balance, funded, "a retired title is never charged for");
}

#[tokio::test]
async fn title_rental_replaces_expires_and_leaves_the_username_effect_alone() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "title-rental-buy").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        CUSTOM_TITLE_MONTH_PRICE + CUSTOM_TITLE_DAY_PRICE + USERNAME_GLOW_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    let before = chrono::Utc::now();
    let row = purchase_item_by_sku_with_custom_title(
        &mut client,
        user.id,
        "title_custom_day",
        CustomTitle::parse("the insufferable").expect("valid title"),
    )
    .await
    .expect("rent title")
    .title_rental
    .expect("activated title row");
    assert_eq!(row.effect_kind, TITLE_EFFECT_KIND);
    assert_eq!(row.room_id, None);
    assert_eq!(row.source_sku, "title_custom_day");
    assert_eq!(
        title_from_payload(&row.payload).as_deref(),
        Some("the insufferable")
    );
    let expected_end = before + chrono::Duration::seconds(RENTAL_DAY_SECS);
    assert!(row.ends_at >= expected_end - chrono::Duration::seconds(60));

    // A color effect is a different slot: buying one leaves the title alone.
    purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Ember),
    )
    .await
    .expect("buy glow");
    assert_eq!(
        active_effect_rows(&client, user.id, TITLE_EFFECT_KIND)
            .await
            .len(),
        1
    );

    // A second title replaces the first, month over day; the color effect is
    // still live.
    let row = purchase_item_by_sku_with_custom_title(
        &mut client,
        user.id,
        "title_custom_month",
        CustomTitle::parse("the night clerk").expect("valid title"),
    )
    .await
    .expect("rent second title")
    .title_rental
    .expect("activated title row");
    let rows = active_effect_rows(&client, user.id, TITLE_EFFECT_KIND).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, row.id);
    assert_eq!(rows[0].source_sku, "title_custom_month");
    assert_eq!(
        title_from_payload(&rows[0].payload).as_deref(),
        Some("the night clerk")
    );
    assert_eq!(
        active_effect_rows(&client, user.id, USERNAME_EFFECT_KIND)
            .await
            .len(),
        1
    );

    // The title lapses on its own clock and leaves the color running.
    expire_effect_rows(&client, user.id, TITLE_EFFECT_KIND).await;
    assert!(
        active_effect_rows(&client, user.id, TITLE_EFFECT_KIND)
            .await
            .is_empty()
    );
    assert_eq!(
        active_effect_rows(&client, user.id, USERNAME_EFFECT_KIND)
            .await
            .len(),
        1
    );
}

#[tokio::test]
async fn seeded_catalog_contains_custom_title_tiers() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let custom: Vec<&MarketplaceItem> = items
        .iter()
        .filter(|item| item.item_kind == TITLE_RENTAL_ITEM_KIND)
        .filter(|item| is_custom_title(&item.payload))
        .collect();
    assert_eq!(custom.len(), 2, "one day tier and one month tier");

    let day = custom
        .iter()
        .find(|item| item.sku == "title_custom_day")
        .expect("custom title, day tier");
    let month = custom
        .iter()
        .find(|item| item.sku == "title_custom_month")
        .expect("custom title, month tier");
    assert_eq!(day.price_chips, CUSTOM_TITLE_DAY_PRICE);
    assert_eq!(month.price_chips, CUSTOM_TITLE_MONTH_PRICE);
    assert_eq!(rental_duration_secs(day), RENTAL_DAY_SECS);
    assert_eq!(rental_duration_secs(month), RENTAL_MONTH_SECS);
    assert_eq!(month.sort_order, day.sort_order + 5);
    for item in &custom {
        assert_eq!(item.name, "Your Own Title");
        assert_eq!(item.slot, None);
        // The text does not exist until someone types it, so nothing can read
        // one out of the payload.
        assert_eq!(title_from_payload(&item.payload), None);
    }
}

#[tokio::test]
async fn custom_title_purchase_wears_the_buyers_collapsed_text() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "custom-title-buy").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        CUSTOM_TITLE_DAY_PRICE,
        None,
    )
    .await
    .expect("fund chips");
    let funded = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;

    let row = purchase_item_by_sku_with_custom_title(
        &mut client,
        user.id,
        "title_custom_day",
        CustomTitle::parse("  the  wrong hour ").expect("valid title"),
    )
    .await
    .expect("rent custom title")
    .title_rental
    .expect("activated custom title");
    assert_eq!(row.effect_kind, TITLE_EFFECT_KIND);
    assert_eq!(row.room_id, None);
    assert_eq!(row.source_sku, "title_custom_day");

    // The live row wears the collapsed text the buyer typed, inside the cap.
    let rows = active_effect_rows(&client, user.id, TITLE_EFFECT_KIND).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, row.id);
    assert_eq!(
        title_from_payload(&rows[0].payload).as_deref(),
        Some("the wrong hour")
    );
    assert!(
        title_from_payload(&rows[0].payload)
            .unwrap()
            .chars()
            .count()
            <= TITLE_MAX_LEN
    );

    let balance = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;
    assert_eq!(balance, funded - CUSTOM_TITLE_DAY_PRICE);
}

#[tokio::test]
async fn a_custom_title_bought_without_text_charges_nobody() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "custom-title-mismatch").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        CUSTOM_TITLE_MONTH_PRICE,
        None,
    )
    .await
    .expect("fund chips");
    let funded = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;

    // The custom SKU bought through the plain path carries no text at all, so
    // the transaction fails rather than activating an empty title.
    assert!(
        purchase_item_by_sku_with_chat_effect(&mut client, user.id, "title_custom_day", None)
            .await
            .is_err()
    );

    assert!(
        active_effect_rows(&client, user.id, TITLE_EFFECT_KIND)
            .await
            .is_empty()
    );
    let balance = UserChips::ensure(&client, user.id)
        .await
        .expect("balance")
        .balance;
    assert_eq!(balance, funded, "a refused title is never charged for");
}

const USERNAME_GLOW_PRICE: i64 = 200;
const USERNAME_GRADIENT_PRICE: i64 = 500;
const USERNAME_SHIMMER_PRICE: i64 = 1_000;
/// The month tier is 40x the day tier (migration 153): a convenience premium.
const USERNAME_MONTH_PRICE_MULTIPLIER: i64 = 40;

#[tokio::test]
async fn seeded_catalog_contains_username_effects() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let expectations = [
        (USERNAME_GLOW_SKU, "Name Glow", USERNAME_GLOW_PRICE, "glow"),
        (
            USERNAME_GRADIENT_SKU,
            "Name Gradient",
            USERNAME_GRADIENT_PRICE,
            "gradient",
        ),
        (
            USERNAME_SHIMMER_SKU,
            "Name Shimmer",
            USERNAME_SHIMMER_PRICE,
            "shimmer",
        ),
    ];
    for (sku, name, price, variant) in expectations {
        let item = items
            .iter()
            .find(|item| item.sku == sku)
            .unwrap_or_else(|| panic!("missing {sku}"));
        assert_eq!(item.item_kind, USERNAME_EFFECT_ITEM_KIND);
        assert_eq!(item.name, name);
        assert_eq!(item.price_chips, price);
        assert_eq!(item.payload["variant"], variant);
        assert_eq!(item.payload["duration_secs"], 86_400);
        assert_eq!(rental_duration_secs(item), 86_400);
        assert!(item.active);
    }
}

#[tokio::test]
async fn seeded_catalog_contains_monthly_username_effects() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let expectations = [
        (
            USERNAME_GLOW_MONTH_SKU,
            USERNAME_GLOW_SKU,
            "Name Glow Monthly",
            USERNAME_GLOW_PRICE,
            "glow",
        ),
        (
            USERNAME_GRADIENT_MONTH_SKU,
            USERNAME_GRADIENT_SKU,
            "Name Gradient Monthly",
            USERNAME_GRADIENT_PRICE,
            "gradient",
        ),
        (
            USERNAME_SHIMMER_MONTH_SKU,
            USERNAME_SHIMMER_SKU,
            "Name Shimmer Monthly",
            USERNAME_SHIMMER_PRICE,
            "shimmer",
        ),
    ];
    for (sku, day_sku, name, day_price, variant) in expectations {
        let item = items
            .iter()
            .find(|item| item.sku == sku)
            .unwrap_or_else(|| panic!("missing {sku}"));
        let day_item = items
            .iter()
            .find(|item| item.sku == day_sku)
            .unwrap_or_else(|| panic!("missing {day_sku}"));
        assert_eq!(item.item_kind, USERNAME_EFFECT_ITEM_KIND);
        assert_eq!(item.name, name);
        assert_eq!(
            item.price_chips,
            day_price * USERNAME_MONTH_PRICE_MULTIPLIER
        );
        assert_eq!(item.payload["variant"], variant);
        assert_eq!(rental_duration_secs(item), RENTAL_MONTH_SECS);
        assert!(item.active);
        // The month item lists directly under its day twin.
        assert_eq!(item.sort_order, day_item.sort_order + 5);
    }
}

async fn active_username_effect_rows(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
) -> Vec<ShopConsumableEffect> {
    ShopConsumableEffect::active_user_effects(client, USERNAME_EFFECT_KIND)
        .await
        .expect("active effects")
        .into_iter()
        .filter(|row| row.user_id == user_id)
        .collect()
}

#[tokio::test]
async fn username_effect_purchase_debits_and_activates_one_row() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-buy").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row")
        .balance;

    let before = chrono::Utc::now();
    let result = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Ember),
    )
    .await
    .expect("purchase");
    let purchase = result.purchase.expect("item available");
    assert_eq!(purchase.status, PurchaseStatus::Purchased);
    assert_eq!(purchase.balance, starting_balance - USERNAME_GLOW_PRICE);

    let row = result.username_effect.expect("activated effect row");
    assert_eq!(row.user_id, user.id);
    assert_eq!(row.room_id, None);
    assert_eq!(row.effect_kind, USERNAME_EFFECT_KIND);
    assert_eq!(row.source_sku, USERNAME_GLOW_SKU);
    assert_eq!(
        UsernameEffect::from_payload(&row.payload),
        Some(UsernameEffect::Glow(GlowColor::Ember))
    );
    let expected_end = before + chrono::Duration::seconds(86_400);
    assert!(row.ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(row.ends_at <= expected_end + chrono::Duration::seconds(60));

    let rows = active_username_effect_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    let for_user =
        ShopConsumableEffect::active_user_effect_for_user(&client, user.id, USERNAME_EFFECT_KIND)
            .await
            .expect("query")
            .expect("live effect");
    assert_eq!(for_user.id, row.id);
}

#[tokio::test]
async fn monthly_username_effect_purchase_runs_for_thirty_days() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-month").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        // The month price plus the day buy that precedes it.
        USERNAME_GLOW_PRICE * (USERNAME_MONTH_PRICE_MULTIPLIER + 1),
        None,
    )
    .await
    .expect("fund chips");

    // A live day effect first: the month buy has to replace it, not stack.
    purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Ember),
    )
    .await
    .expect("day buy");

    let before = chrono::Utc::now();
    let row = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_MONTH_SKU,
        UsernameEffect::Glow(GlowColor::Sky),
    )
    .await
    .expect("month buy")
    .username_effect
    .expect("activated effect row");

    assert_eq!(row.source_sku, USERNAME_GLOW_MONTH_SKU);
    assert_eq!(
        UsernameEffect::from_payload(&row.payload),
        Some(UsernameEffect::Glow(GlowColor::Sky))
    );
    let expected_end = before + chrono::Duration::seconds(RENTAL_MONTH_SECS);
    assert!(row.ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(row.ends_at <= expected_end + chrono::Duration::seconds(60));

    let rows = active_username_effect_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, row.id);
}

#[tokio::test]
async fn username_effect_rebuy_replaces_the_live_effect() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-rebuy").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        USERNAME_GLOW_PRICE * 2 + USERNAME_GRADIENT_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    let first = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Ember),
    )
    .await
    .expect("first buy")
    .username_effect
    .expect("first row");

    // Same item, new color: exactly one live row, fresh clock.
    let second = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Sky),
    )
    .await
    .expect("second buy")
    .username_effect
    .expect("second row");
    let rows = active_username_effect_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, second.id);
    assert!(second.ends_at >= first.ends_at);

    // Different effect item: still one live row (one active effect per user).
    let third = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GRADIENT_SKU,
        UsernameEffect::Gradient(GradientPair::Ocean),
    )
    .await
    .expect("third buy")
    .username_effect
    .expect("third row");
    let rows = active_username_effect_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, third.id);
    assert_eq!(
        UsernameEffect::from_payload(&rows[0].payload),
        Some(UsernameEffect::Gradient(GradientPair::Ocean))
    );
}

#[tokio::test]
async fn username_effect_expired_rows_are_excluded_from_active_queries() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-expired").await;
    let mut client = test_db.db.get().await.expect("db client");

    purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Lime),
    )
    .await
    .expect("buy");
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET ends_at = current_timestamp - interval '1 minute'
             WHERE user_id = $1 AND effect_kind = $2",
            &[&user.id, &USERNAME_EFFECT_KIND],
        )
        .await
        .expect("force expiry");

    assert!(
        active_username_effect_rows(&client, user.id)
            .await
            .is_empty()
    );
    assert!(
        ShopConsumableEffect::active_user_effect_for_user(&client, user.id, USERNAME_EFFECT_KIND)
            .await
            .expect("query")
            .is_none()
    );

    // Rebuying after natural expiry deactivates the stale row, so expired
    // effects do not accumulate in the active partial index.
    purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Glow(GlowColor::Lime),
    )
    .await
    .expect("rebuy");
    let stale_active: i64 = client
        .query_one(
            "SELECT count(*)
             FROM shop_consumable_effects
             WHERE user_id = $1
               AND effect_kind = $2
               AND active = true
               AND ends_at <= current_timestamp",
            &[&user.id, &USERNAME_EFFECT_KIND],
        )
        .await
        .expect("stale count")
        .get(0);
    assert_eq!(stale_active, 0);
    assert_eq!(active_username_effect_rows(&client, user.id).await.len(), 1);
}

#[tokio::test]
async fn username_effect_mismatched_style_fails_without_charging() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-mismatch").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row")
        .balance;

    // A gradient choice against the glow item aborts the transaction.
    let error = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_GLOW_SKU,
        UsernameEffect::Gradient(GradientPair::Dusk),
    )
    .await
    .expect_err("mismatched variant must fail");
    let message = error.to_string();
    assert!(
        message.starts_with(char::is_lowercase),
        "error should be lowercase: {message}"
    );

    let chips = UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    assert_eq!(
        chips.balance, starting_balance,
        "failed buy must not charge"
    );
    assert!(
        active_username_effect_rows(&client, user.id)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn username_effect_insufficient_funds_creates_no_effect_row() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "username-effect-broke").await;
    let mut client = test_db.db.get().await.expect("db client");

    // The initial grant covers exactly one shimmer; the second buy is broke.
    let first = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_SHIMMER_SKU,
        UsernameEffect::Shimmer,
    )
    .await
    .expect("first buy");
    assert_eq!(
        first.purchase.expect("item").status,
        PurchaseStatus::Purchased
    );

    let second = purchase_item_by_sku_with_username_effect(
        &mut client,
        user.id,
        USERNAME_SHIMMER_SKU,
        UsernameEffect::Shimmer,
    )
    .await
    .expect("second buy");
    let purchase = second.purchase.expect("item");
    assert_eq!(purchase.status, PurchaseStatus::InsufficientFunds);
    assert!(second.username_effect.is_none());
    // The first effect stays live; the failed rebuy neither reset nor cleared it.
    assert_eq!(active_username_effect_rows(&client, user.id).await.len(), 1);
}

const BONSAI_DECAY_SHIELD_PRICE: i64 = 2_000;
const BONSAI_DECAY_SHIELD_DURATION_SECS: i64 = 1_209_600; // 14 days

async fn active_bonsai_decay_protection_rows(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
) -> Vec<ShopConsumableEffect> {
    ShopConsumableEffect::active_user_effects(client, BONSAI_DECAY_PROTECTION_KIND)
        .await
        .expect("active effects")
        .into_iter()
        .filter(|row| row.user_id == user_id)
        .collect()
}

#[tokio::test]
async fn seeded_catalog_contains_bonsai_decay_shield() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");
    let shield = items
        .iter()
        .find(|item| item.sku == BONSAI_DECAY_SHIELD_SKU)
        .expect("bonsai decay shield item");

    assert_eq!(shield.item_kind, BONSAI_CONSUMABLE_ITEM_KIND);
    assert_eq!(shield.name, "Bonsai Decay Shield");
    assert_eq!(shield.price_chips, BONSAI_DECAY_SHIELD_PRICE);
    assert_eq!(shield.payload["effect_kind"], BONSAI_DECAY_PROTECTION_KIND);
    assert_eq!(
        shield.payload["duration_secs"],
        BONSAI_DECAY_SHIELD_DURATION_SECS
    );
    assert!(shield.active);
}

#[tokio::test]
async fn bonsai_decay_shield_purchase_debits_and_activates_a_two_week_window() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-shield-buy").await;
    let mut client = test_db.db.get().await.expect("db client");
    let starting_balance = UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BONSAI_DECAY_SHIELD_PRICE,
        None,
    )
    .await
    .expect("fund chips")
    .expect("credited")
    .balance;

    let before = chrono::Utc::now();
    let result = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("purchase")
        .expect("item available");
    assert_eq!(result.status, PurchaseStatus::Purchased);
    assert_eq!(result.balance, starting_balance - BONSAI_DECAY_SHIELD_PRICE);

    let rows = active_bonsai_decay_protection_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source_sku, BONSAI_DECAY_SHIELD_SKU);
    let expected_end = before + chrono::Duration::seconds(BONSAI_DECAY_SHIELD_DURATION_SECS);
    assert!(rows[0].ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(rows[0].ends_at <= expected_end + chrono::Duration::seconds(60));
}

#[tokio::test]
async fn bonsai_decay_shield_is_repeatable_and_repeated_use_grows_quantity() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-shield-repeat").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BONSAI_DECAY_SHIELD_PRICE * 2,
        None,
    )
    .await
    .expect("fund chips");

    let first = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("first buy")
        .expect("item available");
    assert_eq!(first.status, PurchaseStatus::Purchased);
    assert_eq!(first.quantity, 1);

    let second = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("second buy")
        .expect("item available");
    assert_eq!(second.status, PurchaseStatus::QuantityAdded);
    assert_eq!(second.quantity, 2);
}

#[tokio::test]
async fn bonsai_decay_shield_rebuy_extends_the_live_window_instead_of_resetting_it() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-shield-extend").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BONSAI_DECAY_SHIELD_PRICE * 2,
        None,
    )
    .await
    .expect("fund chips");

    let first = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("first buy")
        .expect("item available");
    assert_eq!(first.status, PurchaseStatus::Purchased);
    let first_rows = active_bonsai_decay_protection_rows(&client, user.id).await;
    assert_eq!(first_rows.len(), 1);
    let first_starts_at = first_rows[0].starts_at;
    let first_ends_at = first_rows[0].ends_at;

    let second = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("second buy")
        .expect("item available");
    assert_eq!(second.status, PurchaseStatus::QuantityAdded);
    let second_rows = active_bonsai_decay_protection_rows(&client, user.id).await;

    // Stacking never discards paid-for time: exactly one live row, whose
    // expiry moved a full 14 days past the first purchase's expiry rather
    // than resetting to 14 days from now.
    assert_eq!(second_rows.len(), 1);
    let expected_end = first_ends_at + chrono::Duration::seconds(BONSAI_DECAY_SHIELD_DURATION_SECS);
    assert!(second_rows[0].ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(second_rows[0].ends_at <= expected_end + chrono::Duration::seconds(60));
    // The window's start carries forward from the first purchase rather
    // than resetting to the rebuy time, so protection credit for the days
    // already covered by the first purchase is never lost.
    assert_eq!(second_rows[0].starts_at, first_starts_at);
}

#[tokio::test]
async fn bonsai_decay_shield_rebuy_after_expiry_starts_a_fresh_window_from_now() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-shield-after-expiry").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BONSAI_DECAY_SHIELD_PRICE * 2,
        None,
    )
    .await
    .expect("fund chips");

    let first = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("first buy")
        .expect("item available");
    assert_eq!(first.status, PurchaseStatus::Purchased);
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET ends_at = current_timestamp - interval '1 minute'
             WHERE user_id = $1 AND effect_kind = $2",
            &[&user.id, &BONSAI_DECAY_PROTECTION_KIND],
        )
        .await
        .expect("force expiry");

    let before = chrono::Utc::now();
    let second = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("second buy")
        .expect("item available");
    assert_eq!(second.status, PurchaseStatus::QuantityAdded);

    let rows = active_bonsai_decay_protection_rows(&client, user.id).await;
    assert_eq!(rows.len(), 1);
    let expected_end = before + chrono::Duration::seconds(BONSAI_DECAY_SHIELD_DURATION_SECS);
    assert!(rows[0].ends_at >= expected_end - chrono::Duration::seconds(60));
    assert!(rows[0].ends_at <= expected_end + chrono::Duration::seconds(60));
    // The row also does not carry forward the lapsed row's starts_at: the
    // gap between the old expiry and this rebuy was genuinely unprotected,
    // so it must not be credited.
    assert!(rows[0].starts_at >= before - chrono::Duration::seconds(60));
}

#[tokio::test]
async fn bonsai_decay_shield_expired_rows_are_excluded_from_active_queries() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-shield-expired").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        BONSAI_DECAY_SHIELD_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    let purchase = purchase_durable_item_by_sku(&mut client, user.id, BONSAI_DECAY_SHIELD_SKU)
        .await
        .expect("buy")
        .expect("item available");
    assert_eq!(purchase.status, PurchaseStatus::Purchased);
    client
        .execute(
            "UPDATE shop_consumable_effects
             SET ends_at = current_timestamp - interval '1 minute'
             WHERE user_id = $1 AND effect_kind = $2",
            &[&user.id, &BONSAI_DECAY_PROTECTION_KIND],
        )
        .await
        .expect("force expiry");

    assert_eq!(
        active_bonsai_decay_protection_rows(&client, user.id)
            .await
            .len(),
        0
    );
}
