use crate::app::hub::shop::state::ShopState;
use crate::app::hub::shop::svc::{ShopCatalogItem, ShopSnapshot};
use crate::app::ultimates::*;
use late_core::models::marketplace::{THEMATRIX_ULTIMATE_SKU, WONDERLAND_ULTIMATE_SKU};

#[test]
fn owned_ultimates_returns_only_owned_ultimate_items() {
    let shop = ShopState::for_test_snapshot(ShopSnapshot {
        items: vec![
            item(WONDERLAND_ULTIMATE_SKU, "ultimate_spell", true),
            item(THEMATRIX_ULTIMATE_SKU, "ultimate_spell", true),
            item("ultimate_locked", "ultimate_spell", false),
            item("badge_cat", "badge", true),
        ],
        ..ShopSnapshot::default()
    });

    let ultimates = owned_ultimates(&shop);

    assert_eq!(ultimates.len(), 2);
    assert_eq!(ultimates[0].sku, WONDERLAND_ULTIMATE_SKU);
    assert_eq!(ultimates[1].sku, THEMATRIX_ULTIMATE_SKU);
}

#[test]
fn new_cast_replaces_active_ultimate_effect() {
    let mut state = UltimateState::default();

    state.apply_cast(&UltimateCast {
        ultimate_id: "wonderland".to_string(),
        seed: 1,
        duration_ms: 10_000,
    });
    state.apply_cast(&UltimateCast {
        ultimate_id: "thematrix".to_string(),
        seed: 2,
        duration_ms: 10_000,
    });

    let effects = state.active_theme_effects();

    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].kind, effects::UltimateEffectKind::Thematrix);
    assert_eq!(effects[0].seed, 2);
}

fn item(sku: &str, item_kind: &str, owned: bool) -> ShopCatalogItem {
    ShopCatalogItem {
        sku: sku.to_string(),
        item_kind: item_kind.to_string(),
        slot: None,
        name: sku.to_string(),
        description: String::new(),
        price_chips: 0,
        owned,
        equipped: false,
        quantity: 0,
        active_quantity: 0,
        remaining_uses: None,
        badge_emoji: None,
        badge_tier: None,
        aquarium_creature: None,
        aquarium_size: None,
        consumable_category: None,
        effect_kind: None,
        requires_room: false,
        daily_limited: false,
        username_effect_variant: None,
    }
}

#[test]
fn cooldown_label_is_minute_granularity() {
    use std::time::Duration;
    assert_eq!(format_cooldown(Duration::from_secs(30)), "<1m");
    assert_eq!(format_cooldown(Duration::from_secs(90)), "1m");
    assert_eq!(format_cooldown(Duration::from_secs(3660)), "1h 1m");
}
