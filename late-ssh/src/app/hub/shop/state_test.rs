use super::*;

use crate::app::common::primitives::BannerKind;
use late_core::models::rental::{RENTAL_DAY_SECS, RENTAL_MONTH_SECS, TITLE_MAX_LEN};

fn make_state() -> ShopState {
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 0,
        items: Vec::new(),
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
        active_bonsai_decay_protection: None,
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
    assert_eq!(state.selected_category(), ShopCategory::Flags);
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
        rental_duration_secs: Some(RENTAL_DAY_SECS),
        badge_slot: None,
        custom_title: false,
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
        active_bonsai_decay_protection: None,
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
        rental_duration_secs: None,
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
        active_bonsai_decay_protection: None,
        active_badge_rental: None,
        active_flag_rental: None,
        active_title: None,
        chat_label_badge: None,
        chat_label_flag: None,
        custom_titles_available: true,
    };
    let state = ShopState::for_test_snapshot(snapshot);
    let skus: Vec<&str> = state
        .visible_items()
        .iter()
        .map(|item| item.sku.as_str())
        .collect();
    assert_eq!(skus, vec!["username_glow_day", "chat_confetti"]);
}

/// The one title the Shop sells: the buyer writes the text, so the catalog
/// row carries none.
fn custom_title_item() -> ShopCatalogItem {
    ShopCatalogItem {
        sku: "title_custom_day".to_string(),
        item_kind: "title_rental".to_string(),
        name: "Your Own Title".to_string(),
        price_chips: 2_000,
        consumable_category: None,
        effect_kind: None,
        username_effect_variant: None,
        rental_duration_secs: Some(RENTAL_DAY_SECS),
        custom_title: true,
        ..glow_item()
    }
}

fn snapshot_with(items: Vec<ShopCatalogItem>) -> ShopSnapshot {
    ShopSnapshot {
        user_id: None,
        balance: 10_000,
        items,
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
        active_bonsai_decay_protection: None,
        active_badge_rental: None,
        active_flag_rental: None,
        active_title: None,
        chat_label_badge: None,
        chat_label_flag: None,
        custom_titles_available: true,
    }
}

#[test]
fn visible_chat_items_put_titles_under_username_effects() {
    let confetti = ShopCatalogItem {
        sku: "chat_confetti".to_string(),
        item_kind: "chat_consumable".to_string(),
        username_effect_variant: None,
        rental_duration_secs: None,
        ..glow_item()
    };
    let state = ShopState::for_test_snapshot(snapshot_with(vec![
        confetti,
        custom_title_item(),
        glow_item(),
    ]));
    let skus: Vec<&str> = state
        .visible_items()
        .iter()
        .map(|item| item.sku.as_str())
        .collect();
    assert_eq!(
        skus,
        vec!["username_glow_day", "title_custom_day", "chat_confetti"]
    );
}

#[test]
fn the_own_chat_badge_joins_the_label_query_flag_first() {
    let mut snapshot = snapshot_with(Vec::new());
    assert_eq!(
        ShopState::for_test_snapshot(snapshot.clone()).equipped_chat_badge(),
        None
    );

    snapshot.chat_label_badge = Some("🐱".to_string());
    assert_eq!(
        ShopState::for_test_snapshot(snapshot.clone()).equipped_chat_badge(),
        Some("🐱".to_string())
    );

    snapshot.chat_label_flag = Some("🇵🇱".to_string());
    assert_eq!(
        ShopState::for_test_snapshot(snapshot).equipped_chat_badge(),
        Some("🇵🇱 🐱".to_string())
    );
}

#[test]
fn expired_rentals_prune_and_flag_change() {
    let past = Utc::now() - chrono::Duration::seconds(1);
    let future = Utc::now() + chrono::Duration::hours(1);
    let rental = |ends_at| ActiveRental {
        label: "🐱".to_string(),
        source_sku: "badge_cat_day".to_string(),
        ends_at,
    };
    let mut snapshot = snapshot_with(Vec::new());
    snapshot.active_badge_rental = Some(rental(past));
    snapshot.active_flag_rental = Some(rental(future));
    snapshot.active_title = Some(rental(past));

    let mut state = ShopState::for_test_snapshot(snapshot);
    let tick = state.tick();
    assert!(tick.snapshot_changed);
    assert!(state.active_badge_rental().is_none());
    assert!(state.active_title().is_none());
    assert!(
        state.active_flag_rental().is_some(),
        "a rental that has not lapsed stays"
    );

    // Nothing left to prune: a second tick reports no change.
    assert!(!state.tick().snapshot_changed);
}

#[test]
fn username_effect_picker_carries_the_bought_tier_duration() {
    let month = ShopCatalogItem {
        sku: "username_glow_month".to_string(),
        name: "Name Glow Monthly".to_string(),
        price_chips: 6_000,
        rental_duration_secs: Some(RENTAL_MONTH_SECS),
        badge_slot: None,
        ..glow_item()
    };
    let snapshot = ShopSnapshot {
        user_id: None,
        balance: 10_000,
        items: vec![month],
        entitlements: ShopEntitlements::default(),
        active_room_effects: HashMap::new(),
        aquarium_hungry: false,
        active_username_effect: None,
        active_bonsai_decay_protection: None,
        active_badge_rental: None,
        active_flag_rental: None,
        active_title: None,
        chat_label_badge: None,
        chat_label_flag: None,
        custom_titles_available: true,
    };
    let mut state = ShopState::for_test_snapshot(snapshot);
    state.activate_selected(None);

    let pending = state.pending_username_effect().expect("picker armed");
    assert_eq!(pending.sku, "username_glow_month");
    assert_eq!(pending.duration_secs, RENTAL_MONTH_SECS);
    // Same styles as the day tier: only the window and the price move.
    assert_eq!(pending.options.len(), 6);
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

#[test]
fn custom_title_enter_arms_a_prompt_that_stops_at_the_render_cap() {
    let mut state = ShopState::for_test_snapshot(snapshot_with(vec![custom_title_item()]));
    assert!(state.activate_selected(None).is_some());
    let pending = state.pending_custom_title().expect("prompt armed");
    assert_eq!(pending.sku, "title_custom_day");
    assert_eq!(pending.duration_secs, RENTAL_DAY_SECS);
    assert_eq!(pending.input, "");

    for ch in "the last honest night clerk".chars() {
        state.push_custom_title_char(ch);
    }
    let pending = state.pending_custom_title().expect("prompt armed");
    assert_eq!(pending.len(), TITLE_MAX_LEN);
    assert_eq!(pending.input, "the last honest nigh");

    state.backspace_custom_title();
    assert_eq!(
        state.pending_custom_title().expect("prompt armed").input,
        "the last honest nig"
    );
}

#[test]
fn a_custom_title_is_never_sold_without_a_screen_to_check_it() {
    let mut snapshot = snapshot_with(vec![custom_title_item()]);
    snapshot.custom_titles_available = false;
    let mut state = ShopState::for_test_snapshot(snapshot);

    let banner = state.activate_selected(None).expect("banner");
    assert!(matches!(banner.kind, BannerKind::Error), "{banner:?}");
    assert!(
        state.pending_custom_title().is_none(),
        "no prompt opens when nothing can screen the text"
    );
}

#[test]
fn a_blank_custom_title_prompt_sends_nothing_and_stays_open() {
    let mut state = ShopState::for_test_snapshot(snapshot_with(vec![custom_title_item()]));
    state.activate_selected(None);
    state.push_custom_title_char(' ');
    state.push_custom_title_char(' ');

    let banner = state.confirm_pending_custom_title().expect("banner");
    assert!(matches!(banner.kind, BannerKind::Error), "{banner:?}");
    assert!(
        state.pending_custom_title().is_some(),
        "an unfinished title is not a refusal: the prompt stays up"
    );
}

#[test]
fn the_custom_title_prompt_clears_on_cancel_and_category_switch() {
    let mut state = ShopState::for_test_snapshot(snapshot_with(vec![custom_title_item()]));
    state.activate_selected(None);
    assert!(state.cancel_pending_custom_title().is_some());
    assert!(state.pending_custom_title().is_none());

    state.activate_selected(None);
    assert!(state.pending_custom_title().is_some());
    state.select_next_category();
    assert!(state.pending_custom_title().is_none());
}
