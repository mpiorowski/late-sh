use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::{
    catalog::ShopCategory,
    entitlements::ShopEntitlements,
    svc::{ShopCatalogItem, ShopEvent, ShopService, ShopSnapshot},
};

pub struct ShopState {
    user_id: Uuid,
    service: ShopService,
    snapshot_rx: watch::Receiver<ShopSnapshot>,
    event_rx: broadcast::Receiver<ShopEvent>,
    snapshot: ShopSnapshot,
    category_index: usize,
    selected_index: usize,
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
        }
    }

    pub fn tick(&mut self) -> ShopTick {
        let snapshot_changed = self.snapshot_rx.has_changed().unwrap_or(false);
        if snapshot_changed {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.clamp_selection();
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

    pub fn equipped_chat_badge(&self) -> Option<&str> {
        self.snapshot
            .items
            .iter()
            .find(|item| item.is_chat_badge() && item.equipped)
            .and_then(|item| item.badge_emoji.as_deref())
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
        self.category_index = (self.category_index + 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    pub fn select_previous_category(&mut self) {
        self.category_index =
            (self.category_index + ShopCategory::ALL.len() - 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    pub fn activate_selected(&mut self) -> Option<Banner> {
        let item = self.selected_item()?.clone();
        if item.is_aquarium_fish() {
            if !self.snapshot.entitlements.has_aquarium() {
                return Some(Banner::error("Unlock Aquarium before buying fish"));
            }
            self.service.purchase_item_task(self.user_id, item.sku);
            return Some(Banner::success(&format!("Buying {}", item.name)));
        }
        if item.owned {
            if item.equipped {
                if let Some(slot) = item.slot {
                    self.service.unequip_slot_task(self.user_id, slot);
                    return Some(Banner::success("Clearing displayed badge"));
                }
                return Some(Banner::success(&format!("{} already unlocked", item.name)));
            }
            if item.slot.is_some() {
                self.service.equip_item_task(self.user_id, item.sku);
                return Some(Banner::success(&format!("Displaying {}", item.name)));
            }
            return Some(Banner::success(&format!("{} already unlocked", item.name)));
        }

        self.service.purchase_item_task(self.user_id, item.sku);
        Some(Banner::success(&format!("Purchasing {}", item.name)))
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

    fn clamp_selection(&mut self) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }
}
