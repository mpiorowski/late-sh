use crate::{
    models::{
        chips::{ChipMove, UserChips},
        marketplace::{
            MarketplaceItem, THEMATRIX_ULTIMATE_SKU, WONDERLAND_ULTIMATE_SKU,
            purchase_durable_item_by_sku,
        },
        milestone::{MILESTONE_BADGE_ITEM_KIND, MilestoneBadge, emoji_from_payload},
    },
    test_utils::{create_test_user, test_db},
};
use serde_json::json;

const WICK_SKU: &str = "milestone_wick";
const FUSE_SKU: &str = "milestone_fuse";
const FURNACE_SKU: &str = "milestone_furnace";
const WICK_PRICE: i64 = 50_000;
const FUSE_PRICE: i64 = 150_000;
const FURNACE_PRICE: i64 = 500_000;
const ULTIMATE_PRICE: i64 = 1_000_000;

/// SHOP.md's fixed-numbers table quotes this ladder, and the emoji are the
/// product. A change here is a decision, not a refactor.
#[tokio::test]
async fn seeded_catalog_contains_the_three_burn_milestones() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");

    for (sku, name, emoji, price) in [
        (WICK_SKU, "Wick", "\u{1F56F}\u{FE0F}", WICK_PRICE),
        (FUSE_SKU, "Fuse", "\u{1F9E8}", FUSE_PRICE),
        (FURNACE_SKU, "Furnace", "\u{1F30B}", FURNACE_PRICE),
    ] {
        let item = items
            .iter()
            .find(|item| item.sku == sku)
            .unwrap_or_else(|| panic!("{sku} seeded"));
        assert_eq!(item.item_kind, MILESTONE_BADGE_ITEM_KIND);
        assert_eq!(item.name, name);
        assert_eq!(item.price_chips, price);
        assert_eq!(emoji_from_payload(&item.payload).as_deref(), Some(emoji));
        // Never an `equipped_slot` item: a milestone is a fourth glyph, not a
        // badge slot, so nothing about it can be equipped or cleared.
        assert_eq!(item.slot, None);
        assert!(item.active);
    }
}

/// The milestone emoji must not collide with anything already worn beside a
/// name, or the dearest purchase in the shop would read as a rented cat.
#[tokio::test]
async fn milestone_emoji_are_unique_across_the_whole_catalog() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let rows = client
        .query(
            "SELECT item_kind, payload->>'emoji' AS emoji
             FROM marketplace_items
             WHERE COALESCE(payload->>'emoji', '') <> ''",
            &[],
        )
        .await
        .expect("emoji rows");

    let mut milestone_emoji = Vec::new();
    let mut other_emoji = Vec::new();
    for row in &rows {
        let kind: String = row.get("item_kind");
        let emoji: String = row.get("emoji");
        match kind.as_str() {
            MILESTONE_BADGE_ITEM_KIND => milestone_emoji.push(emoji),
            _ => other_emoji.push(emoji),
        }
    }

    assert_eq!(milestone_emoji.len(), 3);
    for emoji in &milestone_emoji {
        assert!(
            !other_emoji.contains(emoji),
            "milestone emoji {emoji} is already sold as another item"
        );
    }
}

/// The whole no-equip-flow decision rests on this: buy two, wear the dearer.
#[tokio::test]
async fn the_dearest_milestone_owned_is_the_one_that_shows() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "milestone-ladder").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        user.id,
        ChipMove::Credit,
        WICK_PRICE + FUSE_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    assert_eq!(
        MilestoneBadge::highest_for_user(&client, user.id)
            .await
            .expect("no milestone"),
        None
    );

    purchase_durable_item_by_sku(&mut client, user.id, WICK_SKU)
        .await
        .expect("purchase wick")
        .expect("wick item");
    assert_eq!(
        MilestoneBadge::highest_for_user(&client, user.id)
            .await
            .expect("wick worn")
            .as_deref(),
        Some("\u{1F56F}\u{FE0F}")
    );

    // Buying up replaces what shows. Buying down could not: there is no
    // cheaper rung left to buy once you hold the dearest.
    purchase_durable_item_by_sku(&mut client, user.id, FUSE_SKU)
        .await
        .expect("purchase fuse")
        .expect("fuse item");
    assert_eq!(
        MilestoneBadge::highest_for_user(&client, user.id)
            .await
            .expect("fuse worn")
            .as_deref(),
        Some("\u{1F9E8}")
    );
}

/// One row per owner in the startup seed, and never another user's glyph.
#[tokio::test]
async fn the_seed_lists_one_milestone_per_owner_and_skips_everyone_else() {
    let test_db = test_db().await;
    let owner = create_test_user(&test_db.db, "milestone-owner").await;
    let bystander = create_test_user(&test_db.db, "milestone-bystander").await;
    let mut client = test_db.db.get().await.expect("db client");
    UserChips::apply(
        &**client,
        owner.id,
        ChipMove::Credit,
        WICK_PRICE + FURNACE_PRICE,
        None,
    )
    .await
    .expect("fund chips");

    purchase_durable_item_by_sku(&mut client, owner.id, WICK_SKU)
        .await
        .expect("purchase wick")
        .expect("wick item");
    purchase_durable_item_by_sku(&mut client, owner.id, FURNACE_SKU)
        .await
        .expect("purchase furnace")
        .expect("furnace item");

    let seeded = MilestoneBadge::highest_for_all(&client)
        .await
        .expect("seed milestones");

    assert_eq!(seeded, vec![(owner.id, "\u{1F30B}".to_string())]);
    assert_eq!(
        MilestoneBadge::highest_for_user(&client, bystander.id)
            .await
            .expect("bystander milestone"),
        None
    );
}

/// The ceiling came down with the milestones: at ten million neither spell
/// ever sold, and the top milestone is half the new price.
#[tokio::test]
async fn the_ultimate_spells_cost_one_million() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let items = MarketplaceItem::list_visible(&client)
        .await
        .expect("list items");

    for sku in [WONDERLAND_ULTIMATE_SKU, THEMATRIX_ULTIMATE_SKU] {
        let item = items
            .iter()
            .find(|item| item.sku == sku)
            .unwrap_or_else(|| panic!("{sku} seeded"));
        assert_eq!(item.price_chips, ULTIMATE_PRICE);
        assert!(item.active);
    }
    assert_eq!(FURNACE_PRICE * 2, ULTIMATE_PRICE);
}

/// A malformed payload renders nothing rather than an empty glyph.
#[test]
fn a_milestone_without_an_emoji_shows_nothing() {
    assert_eq!(emoji_from_payload(&json!({})), None);
    assert_eq!(emoji_from_payload(&json!({ "emoji": "  " })), None);
    assert_eq!(
        emoji_from_payload(&json!({ "emoji": " \u{1F30B} " })).as_deref(),
        Some("\u{1F30B}")
    );
}
