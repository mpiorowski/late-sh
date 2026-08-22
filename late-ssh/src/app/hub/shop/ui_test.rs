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
        username_effect_duration_secs: None,
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
