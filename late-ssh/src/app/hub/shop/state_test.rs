use super::*;

fn make_state() -> ShopState {
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 0,
        items: Vec::new(),
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
    };
    ShopState::for_test_snapshot(snapshot)
}

#[test]
fn category_at_point_hits_set_rect() {
    let state = make_state();
    let mut rects = [Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()];
    rects[0] = Rect::new(2, 3, 12, 1);
    rects[1] = Rect::new(15, 3, 6, 1);
    state.set_category_rects(rects);

    assert_eq!(state.category_at_point(2, 3), Some(0));
    assert_eq!(state.category_at_point(13, 3), Some(0));
    assert_eq!(state.category_at_point(15, 3), Some(1));
    assert_eq!(state.category_at_point(20, 3), Some(1));
    assert_eq!(state.category_at_point(0, 3), None);
    assert_eq!(state.category_at_point(2, 4), None);
}

#[test]
fn item_at_point_hits_set_rect() {
    let state = make_state();
    let rects = vec![
        (Rect::new(2, 5, 40, 1), 0),
        (Rect::new(2, 6, 40, 1), 1),
        (Rect::new(2, 8, 40, 1), 3),
    ];
    state.set_item_rects(rects);

    assert_eq!(state.item_at_point(2, 5), Some(0));
    assert_eq!(state.item_at_point(41, 5), Some(0));
    assert_eq!(state.item_at_point(2, 6), Some(1));
    assert_eq!(state.item_at_point(2, 8), Some(3));
    assert_eq!(state.item_at_point(2, 7), None);
    assert_eq!(state.item_at_point(0, 5), None);
}

#[test]
fn select_category_by_index_switches_and_resets_selection() {
    let mut state = make_state();
    assert_eq!(state.selected_category_index(), 0);
    assert_eq!(state.selected_category(), ShopCategory::Chat);

    state.selected_index = 5;
    state.select_category_by_index(2);

    assert_eq!(state.selected_category_index(), 2);
    assert_eq!(state.selected_category(), ShopCategory::Aquarium);
    assert_eq!(state.selected_index, 0);
    assert!(state.pending_room_effect.is_none());
}

#[test]
fn select_category_by_index_out_of_bounds_is_noop() {
    let mut state = make_state();
    state.select_category_by_index(99);
    assert_eq!(state.selected_category_index(), 0);
}

#[test]
fn select_item_handles_empty_list() {
    let mut state = make_state();
    state.selected_index = 5;
    state.select_item(0);
    assert_eq!(state.selected_index, 0);
}

#[test]
fn set_item_rects_replaces_previous() {
    let state = make_state();
    let first = vec![(Rect::new(0, 0, 10, 1), 0)];
    state.set_item_rects(first);
    assert_eq!(state.item_at_point(5, 0), Some(0));

    let second = Vec::new();
    state.set_item_rects(second);
    assert_eq!(state.item_at_point(5, 0), None);
}

fn glow_item() -> ShopCatalogItem {
    ShopCatalogItem {
        sku: "username_glow_day".to_string(),
        item_kind: "username_effect".to_string(),
        slot: None,
        name: "Name Glow".to_string(),
        description: String::new(),
        price_chips: 200,
        owned: false,
        equipped: false,
        quantity: 0,
        active_quantity: 0,
        remaining_uses: None,
        badge_emoji: None,
        badge_tier: None,
        aquarium_creature: None,
        aquarium_size: None,
        consumable_category: Some("identity".to_string()),
        effect_kind: Some("username_effect".to_string()),
        requires_room: false,
        daily_limited: false,
        username_effect_variant: Some("glow".to_string()),
    }
}

fn make_state_with_glow_item() -> ShopState {
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 1000,
        items: vec![glow_item()],
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
    };
    ShopState::for_test_snapshot(snapshot)
}

#[test]
fn username_effect_enter_arms_picker_and_cycle_wraps() {
    let mut state = make_state_with_glow_item();
    // The Chat tab (index 0) shows username effects.
    assert!(state.activate_selected(None).is_some());
    let pending = state.pending_username_effect().expect("picker armed");
    assert_eq!(pending.sku, "username_glow_day");
    assert_eq!(pending.options.len(), 6);
    assert_eq!(pending.selected, 0);

    state.cycle_pending_username_effect(-1);
    assert_eq!(
        state.pending_username_effect().expect("armed").selected,
        5,
        "cycling left from 0 wraps to the last option"
    );
    state.cycle_pending_username_effect(1);
    assert_eq!(state.pending_username_effect().expect("armed").selected, 0);
}

#[test]
fn username_effect_picker_clears_on_cancel_and_category_switch() {
    let mut state = make_state_with_glow_item();
    state.activate_selected(None);
    assert!(state.pending_username_effect().is_some());
    assert!(state.cancel_pending_username_effect().is_some());
    assert!(state.pending_username_effect().is_none());

    state.activate_selected(None);
    assert!(state.pending_username_effect().is_some());
    state.select_next_category();
    assert!(state.pending_username_effect().is_none());
}

#[test]
fn visible_items_lead_with_username_effects() {
    let confetti = ShopCatalogItem {
        sku: "chat_confetti".to_string(),
        item_kind: "chat_consumable".to_string(),
        username_effect_variant: None,
        ..glow_item()
    };
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 1000,
        items: vec![confetti, glow_item()],
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
    };
    let state = ShopState::for_test_snapshot(snapshot);
    let skus: Vec<&str> = state
        .visible_items()
        .iter()
        .map(|item| item.sku.as_str())
        .collect();
    assert_eq!(skus, vec!["username_glow_day", "chat_confetti"]);
}

#[test]
fn username_effect_options_map_variants() {
    assert_eq!(username_effect_options(Some("glow")).len(), 6);
    assert_eq!(username_effect_options(Some("gradient")).len(), 6);
    assert_eq!(
        username_effect_options(Some("shimmer")),
        vec![UsernameEffect::Shimmer]
    );
    assert!(username_effect_options(Some("sparkle")).is_empty());
    assert!(username_effect_options(None).is_empty());
}

#[test]
fn expired_username_effect_prunes_and_flags_change() {
    let mut state = make_state_with_glow_item();
    state.snapshot.active_username_effect = Some(ActiveUsernameEffect {
        effect: UsernameEffect::Shimmer,
        ends_at: Utc::now() - chrono::Duration::seconds(1),
    });
    assert!(state.prune_expired_effects(Utc::now()));
    assert!(state.snapshot.active_username_effect.is_none());
    // Nothing left to prune: quiet second pass.
    assert!(!state.prune_expired_effects(Utc::now()));
}

#[test]
fn rect_contains_edge_cases() {
    assert!(!rect_contains(Rect::new(0, 0, 0, 1), 0, 0));
    assert!(!rect_contains(Rect::new(0, 0, 1, 0), 0, 0));
    assert!(rect_contains(Rect::new(2, 3, 5, 1), 2, 3));
    assert!(!rect_contains(Rect::new(2, 3, 5, 1), 7, 3));
    assert!(!rect_contains(Rect::new(2, 3, 5, 1), 2, 4));
}
