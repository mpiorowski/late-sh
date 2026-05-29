use std::time::{Duration, Instant};

use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::{
    catalog::ShopCategory,
    entitlements::ShopEntitlements,
    svc::{ShopCatalogItem, ShopEvent, ShopService, ShopSnapshot},
};

/// How long the sender's purchase celebration stays on screen. Long
/// enough to register the sparkle, short enough that it never blocks
/// the next click.
pub const SHOP_PURCHASE_CELEBRATION_DURATION: Duration = Duration::from_secs(3);

/// Sender-side purchase celebration marker — set the moment the shop
/// service confirms a successful purchase, expired on tick after the
/// duration. Drives the pixel-burst overlay over the Shop body for
/// capable terminals; non-capable terminals see only the existing
/// success banner.
#[derive(Debug, Clone, Copy)]
pub struct ShopPurchaseCelebration {
    pub started_at: Instant,
}

impl ShopPurchaseCelebration {
    pub fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= SHOP_PURCHASE_CELEBRATION_DURATION
    }
}

pub struct ShopState {
    user_id: Uuid,
    service: ShopService,
    snapshot_rx: watch::Receiver<ShopSnapshot>,
    event_rx: broadcast::Receiver<ShopEvent>,
    snapshot: ShopSnapshot,
    category_index: usize,
    selected_index: usize,
    /// True after `activate_selected` kicks off a purchase task, cleared
    /// when the matching `ShopEvent::ActionCompleted` or `ActionFailed`
    /// arrives. Used to differentiate purchase completions from equip /
    /// unequip / aquarium-quantity completions so only real first-time
    /// unlocks trigger the pixel celebration.
    pending_purchase: bool,
    /// Active sender-side celebration, if any. Renderer reads via
    /// `purchase_celebration()`; lifecycle is fully owned here.
    purchase_celebration: Option<ShopPurchaseCelebration>,
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
            pending_purchase: false,
            purchase_celebration: None,
        }
    }

    pub fn purchase_celebration(&self) -> Option<ShopPurchaseCelebration> {
        self.purchase_celebration
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
                    if self.pending_purchase {
                        self.pending_purchase = false;
                        self.purchase_celebration = Some(ShopPurchaseCelebration {
                            started_at: Instant::now(),
                        });
                    }
                    banner = Some(Banner::success(&message));
                }
                ShopEvent::ActionFailed { user_id, message } if user_id == self.user_id => {
                    // Drop the pending marker on failure so a later
                    // unrelated completion (an equip, for instance)
                    // doesn't get celebrated as if it were the purchase.
                    self.pending_purchase = false;
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }

        if let Some(celebration) = self.purchase_celebration
            && celebration.is_expired(Instant::now())
        {
            self.purchase_celebration = None;
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
            // First fish of a kind is treated like a purchase — quantity
            // adjustments use `adjust_selected_aquarium_fish` instead and
            // do not arm the celebration marker.
            self.service.purchase_item_task(self.user_id, item.sku);
            self.pending_purchase = true;
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
        self.pending_purchase = true;
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
            pending_purchase: false,
            purchase_celebration: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purchase_celebration_expires_after_duration() {
        let started_at = Instant::now();
        let celebration = ShopPurchaseCelebration { started_at };
        assert!(!celebration.is_expired(started_at));
        assert!(!celebration.is_expired(started_at + SHOP_PURCHASE_CELEBRATION_DURATION / 2));
        assert!(celebration.is_expired(started_at + SHOP_PURCHASE_CELEBRATION_DURATION));
        assert!(
            celebration.is_expired(
                started_at + SHOP_PURCHASE_CELEBRATION_DURATION + Duration::from_secs(1)
            )
        );
    }
}
