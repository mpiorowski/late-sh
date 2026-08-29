use super::*;

use crate::app::hub::shop::entitlements::ShopEntitlements;
use crate::app::hub::shop::svc::ShopSnapshot;
use late_core::models::bonsai_decay_protection::BonsaiDecayProtection;
use late_core::models::marketplace::{BONSAI_CONSUMABLE_ITEM_KIND, BONSAI_DECAY_SHIELD_SKU};
use std::collections::HashMap;

#[test]
fn visible_window_start_keeps_selected_item_visible() {
    assert_eq!(visible_window_start(0, 20, 5), 0);
    assert_eq!(visible_window_start(3, 20, 5), 1);
    assert_eq!(visible_window_start(19, 20, 5), 15);
}

#[test]
fn pad_display_width_handles_variation_selector_emoji() {
    let padded = pad_display_width("☀️", 6);
    assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
    let padded = pad_display_width("🐱", 6);
    assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
}

#[test]
fn remaining_label_floors_at_one_minute() {
    use chrono::{Duration, Utc};
    let now = Utc::now();
    assert_eq!(remaining_label(now + Duration::hours(17), now), "17h left");
    assert_eq!(
        remaining_label(now + Duration::minutes(59), now),
        "59m left"
    );
    assert_eq!(remaining_label(now + Duration::seconds(5), now), "1m left");
    assert_eq!(remaining_label(now - Duration::minutes(3), now), "1m left");
}

#[test]
fn remaining_label_switches_to_days_only_after_a_full_day() {
    use chrono::{Duration, Utc};
    let now = Utc::now();
    assert_eq!(remaining_label(now + Duration::hours(23), now), "23h left");
    // Exactly 24 hours remaining (the day tier's max) must still read "24h
    // left", not flip to "1d left" for the one minute before it drops into
    // the hour tier.
    assert_eq!(remaining_label(now + Duration::days(1), now), "24h left");
    assert_eq!(
        remaining_label(now + Duration::days(1) + Duration::minutes(1), now),
        "1d left"
    );
    assert_eq!(remaining_label(now + Duration::days(14), now), "14d left");
    assert_eq!(remaining_label(now + Duration::days(30), now), "30d left");
}

fn bonsai_shield_item() -> ShopCatalogItem {
    ShopCatalogItem {
        sku: BONSAI_DECAY_SHIELD_SKU.to_string(),
        item_kind: BONSAI_CONSUMABLE_ITEM_KIND.to_string(),
        slot: None,
        name: "Bonsai Decay Shield".to_string(),
        description: String::new(),
        price_chips: 2_000,
        owned: true,
        equipped: false,
        // Every purchase collapses into one running protection window rather
        // than decrementing, so a nonzero lifetime purchase count here must
        // never be read as "unused stock" the way it is for Pet/Aquarium Food.
        quantity: 3,
        active_quantity: 0,
        remaining_uses: None,
        badge_emoji: None,
        badge_tier: None,
        aquarium_creature: None,
        aquarium_size: None,
        consumable_category: Some("bonsai".to_string()),
        effect_kind: Some("bonsai_decay_protection".to_string()),
        requires_room: false,
        daily_limited: false,
        username_effect_variant: None,
        rental_duration_secs: None,
        badge_slot: None,
        custom_title: false,
    }
}

fn make_state_with_bonsai_protection(protection: Option<BonsaiDecayProtection>) -> ShopState {
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 1_000,
        items: vec![bonsai_shield_item()],
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
        active_bonsai_decay_protection: protection,
        active_badge_rental: None,
        active_flag_rental: None,
        active_title: None,
        chat_label_badge: None,
        chat_label_flag: None,
        custom_titles_available: true,
    };
    ShopState::for_test_snapshot(snapshot)
}

#[test]
fn consumable_row_status_reads_active_while_the_shield_is_live() {
    use chrono::{Duration, Utc};
    let item = bonsai_shield_item();

    let live = make_state_with_bonsai_protection(Some(BonsaiDecayProtection {
        starts_at: Utc::now(),
        ends_at: Utc::now() + Duration::days(10),
    }));
    assert_eq!(consumable_row_status(&item, &live), "active");

    let none = make_state_with_bonsai_protection(None);
    assert_eq!(consumable_row_status(&item, &none), "buy");
}

#[test]
fn item_row_hides_the_lifetime_purchase_count_as_stock_for_the_bonsai_shield() {
    let item = bonsai_shield_item();
    let state = make_state_with_bonsai_protection(None);
    let line = item_row(ShopCategory::Companions, false, &item, &state);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        !text.contains("x3"),
        "shop list row must not show the lifetime purchase count as unused stock: {text:?}"
    );
}

/// A Chat tab item of the given kind, nothing owned, nothing active.
fn chat_item(sku: &str, item_kind: &str) -> ShopCatalogItem {
    ShopCatalogItem {
        sku: sku.to_string(),
        item_kind: item_kind.to_string(),
        name: sku.to_string(),
        owned: false,
        quantity: 0,
        consumable_category: None,
        effect_kind: None,
        ..bonsai_shield_item()
    }
}

fn row_labels(rows: &[ItemListRow<'_>]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            ItemListRow::Section(label) => format!("[{label}]"),
            ItemListRow::Item { index, item } => format!("{index}:{}", item.sku),
        })
        .collect()
}

#[test]
fn chat_tab_rows_open_each_group_with_a_section_label() {
    use late_core::models::marketplace::USERNAME_EFFECT_ITEM_KIND;
    use late_core::models::rental::{RENTAL_DAY_SECS, RENTAL_MONTH_SECS, TITLE_RENTAL_ITEM_KIND};

    let glow_day = chat_item("username_glow_day", USERNAME_EFFECT_ITEM_KIND);
    let glow_month = chat_item("username_glow_month", USERNAME_EFFECT_ITEM_KIND);
    let title_day = ShopCatalogItem {
        rental_duration_secs: Some(RENTAL_DAY_SECS),
        custom_title: true,
        ..chat_item("title_custom_day", TITLE_RENTAL_ITEM_KIND)
    };
    let title_month = ShopCatalogItem {
        rental_duration_secs: Some(RENTAL_MONTH_SECS),
        custom_title: true,
        ..chat_item("title_custom_month", TITLE_RENTAL_ITEM_KIND)
    };
    let spark = chat_item("chat_room_spark", CHAT_CONSUMABLE_ITEM_KIND);

    // Items arrive in `visible_items` order: effects, titles, consumables.
    let rows = item_list_rows(
        ShopCategory::Chat,
        &[&glow_day, &glow_month, &title_day, &title_month, &spark],
    );
    assert_eq!(
        row_labels(&rows),
        vec![
            "[Name effects]",
            "0:username_glow_day",
            "1:username_glow_month",
            "[Title]",
            "2:title_custom_day",
            "3:title_custom_month",
            "[Consumables]",
            "4:chat_room_spark",
        ]
    );

    // The tabs without groups list their items bare.
    let rows = item_list_rows(ShopCategory::Flags, &[&glow_day, &spark]);
    assert_eq!(
        row_labels(&rows),
        vec!["0:username_glow_day", "1:chat_room_spark"]
    );
}

#[test]
fn title_rows_tell_the_two_tiers_apart_with_the_duration_tag() {
    use late_core::models::rental::{RENTAL_MONTH_SECS, TITLE_RENTAL_ITEM_KIND};

    let state = make_state_with_bonsai_protection(None);
    let title = ShopCatalogItem {
        name: "Your Own Title".to_string(),
        rental_duration_secs: Some(RENTAL_MONTH_SECS),
        custom_title: true,
        ..chat_item("title_custom_month", TITLE_RENTAL_ITEM_KIND)
    };
    let line = item_row(ShopCategory::Chat, false, &title, &state);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        text.contains("Your Own Title  30d"),
        "title row must carry its tier tag: {text:?}"
    );
}
