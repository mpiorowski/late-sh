use chrono::{DateTime, Utc};
use ratatui::layout::Rect;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::{
    catalog::ShopCategory,
    entitlements::ShopEntitlements,
    svc::{ActiveChatRoomEffect, ShopCatalogItem, ShopEvent, ShopService, ShopSnapshot},
};
use late_core::models::marketplace::{AQUARIUM_FOOD_SKU, CHAT_CONSUMABLE_ITEM_KIND, PET_FOOD_SKU};

pub struct ShopState {
    user_id: Uuid,
    service: ShopService,
    snapshot_rx: watch::Receiver<ShopSnapshot>,
    event_rx: broadcast::Receiver<ShopEvent>,
    snapshot: ShopSnapshot,
    category_index: usize,
    selected_index: usize,
    pending_room_effect: Option<PendingRoomEffect>,
    category_rects: Cell<[Rect; ShopCategory::ALL.len()]>,
    item_rects: RefCell<Vec<(Rect, usize)>>,
}

#[derive(Clone, Debug)]
pub struct RoomEffectTarget {
    pub room_id: Uuid,
    pub label: String,
    pub kind: String,
    pub visibility: String,
    pub permanent: bool,
    pub slug: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PendingRoomEffect {
    pub sku: String,
    pub item_name: String,
    pub price_chips: i64,
    pub effect_kind: Option<String>,
    pub room_id: Uuid,
    pub room_label: String,
    pub daily_limited: bool,
}

pub struct ShopTick {
    pub banner: Option<Banner>,
    pub snapshot_changed: bool,
}

impl ShopState {
    pub fn new(
        user_id: Uuid,
        service: ShopService,
        snapshot_rx: watch::Receiver<ShopSnapshot>,
    ) -> Self {
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = service.subscribe_events();
        Self {
            user_id,
            service,
            snapshot_rx,
            event_rx,
            snapshot,
            category_index: 0,
            selected_index: 0,
            pending_room_effect: None,
            category_rects: Cell::new([Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()]),
            item_rects: RefCell::new(Vec::new()),
        }
    }

    pub fn tick(&mut self) -> ShopTick {
        let mut snapshot_changed = self.snapshot_rx.has_changed().unwrap_or(false);
        if snapshot_changed {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.clamp_selection();
        }
        if self.prune_expired_effects(Utc::now()) {
            snapshot_changed = true;
        }

        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                ShopEvent::ActionCompleted { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::success(&message));
                }
                ShopEvent::ActionFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        ShopTick {
            banner,
            snapshot_changed,
        }
    }

    pub fn balance(&self) -> i64 {
        self.snapshot.balance
    }

    pub fn is_loaded(&self) -> bool {
        self.snapshot.user_id == Some(self.user_id)
    }

    pub fn entitlements(&self) -> &ShopEntitlements {
        &self.snapshot.entitlements
    }

    pub fn all_items(&self) -> &[ShopCatalogItem] {
        &self.snapshot.items
    }

    pub fn selected_category(&self) -> ShopCategory {
        ShopCategory::ALL[self.category_index.min(ShopCategory::ALL.len() - 1)]
    }

    pub fn selected_category_index(&self) -> usize {
        self.category_index
    }

    pub fn visible_items(&self) -> Vec<&ShopCatalogItem> {
        let category = self.selected_category();
        self.snapshot
            .items
            .iter()
            .filter(|item| category.matches_item(item))
            .collect()
    }

    pub fn active_aquarium_fish(&self) -> Vec<(String, usize)> {
        if !self.snapshot.entitlements.has_aquarium() {
            return Vec::new();
        }
        self.snapshot
            .items
            .iter()
            .filter_map(|item| {
                let creature = item.aquarium_creature.as_ref()?;
                (item.active_quantity > 0)
                    .then_some((creature.clone(), item.active_quantity.max(0) as usize))
            })
            .collect()
    }

    pub fn active_room_effects(&self) -> &HashMap<Uuid, Vec<ActiveChatRoomEffect>> {
        &self.snapshot.active_room_effects
    }

    pub fn pending_room_effect(&self) -> Option<&PendingRoomEffect> {
        self.pending_room_effect.as_ref()
    }

    pub fn pet_food_quantity(&self) -> i32 {
        self.snapshot
            .items
            .iter()
            .find(|item| item.sku == PET_FOOD_SKU)
            .map(|item| item.quantity.max(0))
            .unwrap_or(0)
    }

    pub fn aquarium_food_quantity(&self) -> i32 {
        self.snapshot
            .items
            .iter()
            .find(|item| item.sku == AQUARIUM_FOOD_SKU)
            .map(|item| item.quantity.max(0))
            .unwrap_or(0)
    }

    pub fn aquarium_hungry(&self) -> bool {
        self.snapshot.aquarium_hungry
    }

    pub fn equipped_chat_badge(&self) -> Option<String> {
        let mut pieces = Vec::new();
        pieces.extend(
            self.snapshot
                .items
                .iter()
                .filter(|item| item.is_flag_badge() && item.equipped)
                .filter_map(|item| item.badge_emoji.as_deref()),
        );
        pieces.extend(
            self.snapshot
                .items
                .iter()
                .filter(|item| item.is_chat_badge() && !item.is_flag_badge() && item.equipped)
                .filter_map(|item| item.badge_emoji.as_deref()),
        );
        let badge = pieces.join(" ");
        (!badge.is_empty()).then_some(badge)
    }

    pub fn dynamic_bonsai_enabled(&self) -> bool {
        self.snapshot
            .items
            .iter()
            .any(|item| item.is_dynamic_bonsai() && item.equipped)
    }

    pub fn has_dynamic_bonsai(&self) -> bool {
        self.snapshot.entitlements.has_dynamic_bonsai()
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn selected_item(&self) -> Option<&ShopCatalogItem> {
        self.visible_items().get(self.selected_index).copied()
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub fn select_next_category(&mut self) {
        self.pending_room_effect = None;
        self.category_index = (self.category_index + 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    /// Jump to a specific category by value. Used by direct entry points
    /// (e.g. clicking a chat-author store badge to open the shop on Badges)
    /// where stepping with `select_next_category` would be brittle to
    /// `ShopCategory::ALL` reordering.
    pub fn select_category(&mut self, category: ShopCategory) {
        if let Some(idx) = ShopCategory::ALL.iter().position(|c| *c == category) {
            self.category_index = idx;
            self.selected_index = 0;
            self.pending_room_effect = None;
        }
    }

    pub fn select_previous_category(&mut self) {
        self.pending_room_effect = None;
        self.category_index =
            (self.category_index + ShopCategory::ALL.len() - 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    pub fn set_category_rects(&self, rects: [Rect; ShopCategory::ALL.len()]) {
        self.category_rects.set(rects);
    }

    pub fn set_item_rects(&self, rects: Vec<(Rect, usize)>) {
        *self.item_rects.borrow_mut() = rects;
    }

    pub fn category_at_point(&self, x: u16, y: u16) -> Option<usize> {
        let rects = self.category_rects.get();
        rects.iter().enumerate().find_map(|(idx, rect)| {
            if rect_contains(*rect, x, y) {
                Some(idx)
            } else {
                None
            }
        })
    }

    pub fn item_at_point(&self, x: u16, y: u16) -> Option<usize> {
        let rects = self.item_rects.borrow();
        rects.iter().find_map(|(rect, idx)| {
            if rect_contains(*rect, x, y) {
                Some(*idx)
            } else {
                None
            }
        })
    }

    pub fn select_item(&mut self, index: usize) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = index.min(len - 1);
        }
    }

    pub fn select_category_by_index(&mut self, index: usize) {
        if index < ShopCategory::ALL.len() {
            self.category_index = index;
            self.selected_index = 0;
            self.pending_room_effect = None;
        }
    }

    pub fn activate_selected(&mut self, current_room: Option<RoomEffectTarget>) -> Option<Banner> {
        let item = self.selected_item()?.clone();
        let is_dynamic_bonsai = item.is_dynamic_bonsai();
        let current_room_id = current_room.as_ref().map(|room| room.room_id);
        if item.is_aquarium_fish() {
            if !self.snapshot.entitlements.has_aquarium() {
                return Some(Banner::error("Unlock Aquarium before buying fish"));
            }
            self.service
                .purchase_item_task(self.user_id, item.sku, current_room_id);
            return Some(Banner::success(&format!("Buying {}", item.name)));
        }
        if item.is_consumable() {
            if item.requires_room {
                let Some(room) = current_room else {
                    return Some(Banner::error("Open a room before buying this"));
                };
                if item.effect_kind.as_deref() == Some("room_bump") && !room.can_bump() {
                    return Some(Banner::error(
                        "Room Bump only works on public non-permanent topic rooms",
                    ));
                }
                self.pending_room_effect = Some(PendingRoomEffect {
                    sku: item.sku,
                    item_name: item.name,
                    price_chips: item.price_chips,
                    effect_kind: item.effect_kind,
                    room_id: room.room_id,
                    room_label: room.label,
                    daily_limited: item.daily_limited,
                });
                return Some(Banner::success("Confirm room effect"));
            }
            let action = if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND {
                "Activating"
            } else {
                "Buying"
            };
            self.service
                .purchase_item_task(self.user_id, item.sku, current_room_id);
            return Some(Banner::success(&format!("{action} {}", item.name)));
        }
        if item.owned {
            if item.equipped {
                if let Some(slot) = item.slot {
                    self.service.unequip_slot_task(self.user_id, slot);
                    if is_dynamic_bonsai {
                        return Some(Banner::success("Using classic Bonsai"));
                    }
                    return Some(Banner::success("Clearing displayed badge"));
                }
                return Some(Banner::success(&format!("{} already unlocked", item.name)));
            }
            if item.slot.is_some() {
                self.service.equip_item_task(self.user_id, item.sku);
                if is_dynamic_bonsai {
                    return Some(Banner::success("Using Dynamic Bonsai"));
                }
                return Some(Banner::success(&format!("Displaying {}", item.name)));
            }
            return Some(Banner::success(&format!("{} already unlocked", item.name)));
        }

        self.service
            .purchase_item_task(self.user_id, item.sku, current_room_id);
        Some(Banner::success(&format!("Purchasing {}", item.name)))
    }

    pub fn confirm_pending_room_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_room_effect.take()?;
        self.service
            .purchase_item_task(self.user_id, pending.sku, Some(pending.room_id));
        Some(Banner::success(&format!(
            "Activating {} in {}",
            pending.item_name, pending.room_label
        )))
    }

    pub fn cancel_pending_room_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_room_effect.take()?;
        Some(Banner::success(&format!(
            "Cancelled {} for {}",
            pending.item_name, pending.room_label
        )))
    }

    pub fn adjust_selected_aquarium_fish(&mut self, delta: i32) -> Option<Banner> {
        let item = self.selected_item()?.clone();
        if !item.is_aquarium_fish() {
            return None;
        }
        if !self.snapshot.entitlements.has_aquarium() {
            return Some(Banner::error("Unlock Aquarium before managing fish"));
        }
        self.service
            .adjust_aquarium_fish_task(self.user_id, item.sku, delta);
        let label = if delta > 0 { "Adding" } else { "Removing" };
        Some(Banner::success(&format!("{label} {}", item.name)))
    }

    pub fn use_aquarium_food(&mut self) -> Banner {
        if !self.snapshot.entitlements.has_aquarium() {
            return Banner::error("Unlock Aquarium before feeding it");
        }
        if self.aquarium_food_quantity() <= 0 {
            return Banner::error("Buy Aquarium Food first");
        }
        self.service.use_aquarium_food_task(self.user_id);
        Banner::success("Feeding aquarium")
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    fn prune_expired_effects(&mut self, now: DateTime<Utc>) -> bool {
        let mut changed = false;
        self.snapshot.active_room_effects.retain(|_, effects| {
            let before = effects.len();
            effects.retain(|effect| effect.ends_at > now);
            if effects.len() != before {
                changed = true;
            }
            !effects.is_empty()
        });
        changed
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}

impl RoomEffectTarget {
    fn can_bump(&self) -> bool {
        self.kind == "topic"
            && self.visibility == "public"
            && !self.permanent
            && self.slug.as_deref().is_some_and(|slug| !slug.is_empty())
    }
}

#[cfg(test)]
impl ShopState {
    pub(crate) fn for_test_snapshot(snapshot: ShopSnapshot) -> Self {
        let (tx, snapshot_rx) = watch::channel(snapshot.clone());
        drop(tx);
        let service = ShopService::new(
            late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("test db pool"),
        );
        Self {
            user_id: Uuid::nil(),
            service,
            snapshot_rx,
            event_rx: tokio::sync::broadcast::channel(1).1,
            snapshot,
            category_index: 0,
            selected_index: 0,
            pending_room_effect: None,
            category_rects: Cell::new([Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()]),
            item_rects: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> ShopState {
        let snapshot = ShopSnapshot {
            user_id: None,
            balance: 0,
            items: Vec::new(),
            entitlements: ShopEntitlements::default(),
            active_room_effects: HashMap::new(),
            aquarium_hungry: false,
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
        assert_eq!(state.selected_category(), ShopCategory::Companions);

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

    #[test]
    fn rect_contains_edge_cases() {
        assert!(!rect_contains(Rect::new(0, 0, 0, 1), 0, 0));
        assert!(!rect_contains(Rect::new(0, 0, 1, 0), 0, 0));
        assert!(rect_contains(Rect::new(2, 3, 5, 1), 2, 3));
        assert!(!rect_contains(Rect::new(2, 3, 5, 1), 7, 3));
        assert!(!rect_contains(Rect::new(2, 3, 5, 1), 2, 4));
    }
}
