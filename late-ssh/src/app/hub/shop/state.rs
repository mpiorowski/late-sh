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
    svc::{
        ActiveChatRoomEffect, ActiveRental, ActiveUsernameEffect, ShopCatalogItem, ShopEvent,
        ShopService, ShopSnapshot,
    },
};
use late_core::models::{
    bonsai_decay_protection::BonsaiDecayProtection,
    marketplace::{AQUARIUM_FOOD_SKU, CHAT_CONSUMABLE_ITEM_KIND, PET_FOOD_SKU},
    rental::TITLE_MAX_LEN,
    username_effect::{GlowColor, GradientPair, UsernameEffect},
};

pub(crate) struct ShopState {
    user_id: Uuid,
    service: ShopService,
    snapshot_rx: watch::Receiver<ShopSnapshot>,
    event_rx: broadcast::Receiver<ShopEvent>,
    snapshot: ShopSnapshot,
    category_index: usize,
    selected_index: usize,
    pending_room_effect: Option<PendingRoomEffect>,
    pending_username_effect: Option<PendingUsernameEffect>,
    pending_custom_title: Option<PendingCustomTitle>,
    category_rects: Cell<[Rect; ShopCategory::ALL.len()]>,
    item_rects: RefCell<Vec<(Rect, usize)>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RoomEffectTarget {
    pub room_id: Uuid,
    pub label: String,
    pub kind: String,
    pub visibility: String,
    pub permanent: bool,
    pub slug: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingRoomEffect {
    pub sku: String,
    pub item_name: String,
    pub price_chips: i64,
    pub effect_kind: Option<String>,
    pub room_id: Uuid,
    pub room_label: String,
    pub daily_limited: bool,
}

/// The style picker armed by Enter on a username-effect item: cycle through
/// the tier's styles (each swatch previews in its real colors), Enter buys.
#[derive(Clone, Debug)]
pub(crate) struct PendingUsernameEffect {
    pub sku: String,
    pub item_name: String,
    pub price_chips: i64,
    /// How long the bought tier runs, so the confirm modal quotes the window
    /// the buyer is actually paying for.
    pub duration_secs: i64,
    pub options: Vec<UsernameEffect>,
    pub selected: usize,
}

impl PendingUsernameEffect {
    pub(crate) fn selected_effect(&self) -> Option<UsernameEffect> {
        self.options.get(self.selected).copied()
    }
}

/// The text prompt armed by Enter on a custom title: type up to
/// `TITLE_MAX_LEN` characters, Enter sends them to be screened and bought.
/// Same shape as the style picker above, with a line of text where the
/// swatches are.
#[derive(Clone, Debug)]
pub(crate) struct PendingCustomTitle {
    pub sku: String,
    pub price_chips: i64,
    /// How long the bought tier runs, so the prompt quotes the window the
    /// buyer is actually paying for.
    pub duration_secs: i64,
    pub input: String,
}

impl PendingCustomTitle {
    /// What the buyer has typed, with the surrounding whitespace gone. Empty
    /// until there is a title worth screening, which is what gates Enter.
    pub(crate) fn trimmed(&self) -> &str {
        self.input.trim()
    }

    pub(crate) fn len(&self) -> usize {
        self.input.chars().count()
    }
}

/// The pickable styles a username-effect item sells, from its payload
/// variant. Unknown variants sell nothing (and the picker refuses to arm).
fn username_effect_options(variant: Option<&str>) -> Vec<UsernameEffect> {
    match variant {
        Some("glow") => GlowColor::ALL
            .into_iter()
            .map(UsernameEffect::Glow)
            .collect(),
        Some("gradient") => GradientPair::ALL
            .into_iter()
            .map(UsernameEffect::Gradient)
            .collect(),
        Some("shimmer") => vec![UsernameEffect::Shimmer],
        _ => Vec::new(),
    }
}

pub(crate) struct ShopTick {
    pub banner: Option<Banner>,
    pub snapshot_changed: bool,
}

impl ShopState {
    pub(crate) fn new(
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
            pending_username_effect: None,
            pending_custom_title: None,
            category_rects: Cell::new([Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()]),
            item_rects: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn tick(&mut self) -> ShopTick {
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

    pub(crate) fn balance(&self) -> i64 {
        self.snapshot.balance
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.snapshot.user_id == Some(self.user_id)
    }

    pub(crate) fn entitlements(&self) -> &ShopEntitlements {
        &self.snapshot.entitlements
    }

    pub(crate) fn all_items(&self) -> &[ShopCatalogItem] {
        &self.snapshot.items
    }

    pub(crate) fn selected_category(&self) -> ShopCategory {
        ShopCategory::ALL[self.category_index.min(ShopCategory::ALL.len() - 1)]
    }

    pub(crate) fn selected_category_index(&self) -> usize {
        self.category_index
    }

    pub(crate) fn visible_items(&self) -> Vec<&ShopCatalogItem> {
        let category = self.selected_category();
        let mut items: Vec<&ShopCatalogItem> = self
            .snapshot
            .items
            .iter()
            .filter(|item| category.matches_item(item))
            .collect();
        // On the Chat tab the name-adjacent rentals lead: username effects
        // first, then titles, then the room consumables. Stable, so catalog
        // order holds inside each group.
        items.sort_by_key(|item| match item {
            item if item.is_username_effect() => 0,
            item if item.is_title_rental() => 1,
            _ => 2,
        });
        items
    }

    pub(crate) fn active_aquarium_fish(&self) -> Vec<(String, usize)> {
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

    pub(crate) fn active_room_effects(&self) -> &HashMap<Uuid, Vec<ActiveChatRoomEffect>> {
        &self.snapshot.active_room_effects
    }

    pub(crate) fn pending_room_effect(&self) -> Option<&PendingRoomEffect> {
        self.pending_room_effect.as_ref()
    }

    pub(crate) fn pending_username_effect(&self) -> Option<&PendingUsernameEffect> {
        self.pending_username_effect.as_ref()
    }

    pub(crate) fn pending_custom_title(&self) -> Option<&PendingCustomTitle> {
        self.pending_custom_title.as_ref()
    }

    /// Whether the Shop can sell a buyer-written title at all. False means no
    /// screen is configured, and an unscreened title never ships.
    pub(crate) fn custom_titles_available(&self) -> bool {
        self.snapshot.custom_titles_available
    }

    pub(crate) fn active_username_effect(&self) -> Option<ActiveUsernameEffect> {
        self.snapshot.active_username_effect
    }

    pub(crate) fn active_bonsai_decay_protection(&self) -> Option<BonsaiDecayProtection> {
        self.snapshot.active_bonsai_decay_protection
    }

    pub(crate) fn pet_food_quantity(&self) -> i32 {
        self.snapshot
            .items
            .iter()
            .find(|item| item.sku == PET_FOOD_SKU)
            .map(|item| item.quantity.max(0))
            .unwrap_or(0)
    }

    pub(crate) fn aquarium_food_quantity(&self) -> i32 {
        self.snapshot
            .items
            .iter()
            .find(|item| item.sku == AQUARIUM_FOOD_SKU)
            .map(|item| item.quantity.max(0))
            .unwrap_or(0)
    }

    pub(crate) fn aquarium_hungry(&self) -> bool {
        self.snapshot.aquarium_hungry
    }

    pub(crate) fn active_badge_rental(&self) -> Option<&ActiveRental> {
        self.snapshot.active_badge_rental.as_ref()
    }

    pub(crate) fn active_flag_rental(&self) -> Option<&ActiveRental> {
        self.snapshot.active_flag_rental.as_ref()
    }

    pub(crate) fn active_title(&self) -> Option<&ActiveRental> {
        self.snapshot.active_title.as_ref()
    }

    /// The badge string this user's chat label carries, flag first then badge,
    /// exactly as `chat_author_badge` joins them for every other viewer. Comes
    /// straight off the snapshot, which read it from the one query that
    /// resolves the live rentals.
    pub(crate) fn equipped_chat_badge(&self) -> Option<String> {
        let badge = [
            self.snapshot.chat_label_flag.as_deref(),
            self.snapshot.chat_label_badge.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        (!badge.is_empty()).then_some(badge)
    }

    pub(crate) fn dynamic_bonsai_enabled(&self) -> bool {
        self.snapshot
            .items
            .iter()
            .any(|item| item.is_dynamic_bonsai() && item.equipped)
    }

    pub(crate) fn has_dynamic_bonsai(&self) -> bool {
        self.snapshot.entitlements.has_dynamic_bonsai()
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub(crate) fn selected_item(&self) -> Option<&ShopCatalogItem> {
        self.visible_items().get(self.selected_index).copied()
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub(crate) fn select_next_category(&mut self) {
        self.pending_room_effect = None;
        self.pending_username_effect = None;
        self.pending_custom_title = None;
        self.category_index = (self.category_index + 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    /// Jump to a specific category by value. Used by direct entry points
    /// (e.g. clicking a chat-author store badge to open the shop on Badges)
    /// where stepping with `select_next_category` would be brittle to
    /// `ShopCategory::ALL` reordering.
    pub(crate) fn select_category(&mut self, category: ShopCategory) {
        if let Some(idx) = ShopCategory::ALL.iter().position(|c| *c == category) {
            self.category_index = idx;
            self.selected_index = 0;
            self.pending_room_effect = None;
            self.pending_username_effect = None;
            self.pending_custom_title = None;
        }
    }

    pub(crate) fn select_previous_category(&mut self) {
        self.pending_room_effect = None;
        self.pending_username_effect = None;
        self.pending_custom_title = None;
        self.category_index =
            (self.category_index + ShopCategory::ALL.len() - 1) % ShopCategory::ALL.len();
        self.selected_index = 0;
    }

    pub(crate) fn set_category_rects(&self, rects: [Rect; ShopCategory::ALL.len()]) {
        self.category_rects.set(rects);
    }

    pub(crate) fn set_item_rects(&self, rects: Vec<(Rect, usize)>) {
        *self.item_rects.borrow_mut() = rects;
    }

    pub(crate) fn category_at_point(&self, x: u16, y: u16) -> Option<usize> {
        let rects = self.category_rects.get();
        rects.iter().enumerate().find_map(|(idx, rect)| {
            if rect_contains(*rect, x, y) {
                Some(idx)
            } else {
                None
            }
        })
    }

    pub(crate) fn item_at_point(&self, x: u16, y: u16) -> Option<usize> {
        let rects = self.item_rects.borrow();
        rects.iter().find_map(|(rect, idx)| {
            if rect_contains(*rect, x, y) {
                Some(*idx)
            } else {
                None
            }
        })
    }

    pub(crate) fn select_item(&mut self, index: usize) {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = index.min(len - 1);
        }
    }

    pub(crate) fn select_category_by_index(&mut self, index: usize) {
        if index < ShopCategory::ALL.len() {
            self.category_index = index;
            self.selected_index = 0;
            self.pending_room_effect = None;
            self.pending_username_effect = None;
            self.pending_custom_title = None;
        }
    }

    pub(crate) fn activate_selected(
        &mut self,
        current_room: Option<RoomEffectTarget>,
    ) -> Option<Banner> {
        let item = self.selected_item()?.clone();
        let current_room_id = current_room.as_ref().map(|room| room.room_id);
        if item.is_username_effect() {
            let options = username_effect_options(item.username_effect_variant.as_deref());
            if options.is_empty() {
                return Some(Banner::error("This effect is not available"));
            }
            self.pending_username_effect = Some(PendingUsernameEffect {
                duration_secs: item.rental_duration(),
                sku: item.sku,
                item_name: item.name,
                price_chips: item.price_chips,
                options,
                selected: 0,
            });
            return Some(Banner::success("Pick a style"));
        }
        if item.is_aquarium_fish() {
            if !self.snapshot.entitlements.has_aquarium() {
                return Some(Banner::error("Unlock Aquarium before buying fish"));
            }
            self.service
                .purchase_item_task(self.user_id, item.sku, current_room_id, None);
            return Some(Banner::success(&format!("Buying {}", item.name)));
        }
        // A custom title has no text until the buyer writes one, so Enter
        // opens a prompt instead of buying. With no screen configured there is
        // nothing to sell: an unscreened title never ships.
        if item.is_custom_title() {
            if !self.snapshot.custom_titles_available {
                return Some(Banner::error("Custom titles are closed right now"));
            }
            self.pending_custom_title = Some(PendingCustomTitle {
                duration_secs: item.rental_duration(),
                sku: item.sku,
                price_chips: item.price_chips,
                input: String::new(),
            });
            return Some(Banner::success("Write your title"));
        }
        // Rentals are bought outright, every time: there is no picker and no
        // equip step, and a rebuy replaces the live row and resets its clock.
        if item.is_badge_rental() || item.is_title_rental() {
            self.service
                .purchase_item_task(self.user_id, item.sku, None, None);
            return Some(Banner::success(&format!("Renting {}", item.name)));
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
                .purchase_item_task(self.user_id, item.sku, current_room_id, None);
            return Some(Banner::success(&format!("{action} {}", item.name)));
        }
        if item.owned {
            // Dynamic Bonsai is the only thing on sale that still equips a
            // slot. Badges and flags went all-rental in migration 148, and a
            // rental fills its slot through an effect row, never an equip.
            if let Some(slot) = item.slot {
                if item.equipped {
                    self.service.unequip_slot_task(self.user_id, slot);
                    return Some(Banner::success("Using classic Bonsai"));
                }
                self.service.equip_item_task(self.user_id, item.sku);
                return Some(Banner::success("Using Dynamic Bonsai"));
            }
            return Some(Banner::success(&format!("{} already unlocked", item.name)));
        }

        self.service
            .purchase_item_task(self.user_id, item.sku, current_room_id, None);
        Some(Banner::success(&format!("Purchasing {}", item.name)))
    }

    pub(crate) fn confirm_pending_room_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_room_effect.take()?;
        self.service
            .purchase_item_task(self.user_id, pending.sku, Some(pending.room_id), None);
        Some(Banner::success(&format!(
            "Activating {} in {}",
            pending.item_name, pending.room_label
        )))
    }

    pub(crate) fn cancel_pending_room_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_room_effect.take()?;
        Some(Banner::success(&format!(
            "Cancelled {} for {}",
            pending.item_name, pending.room_label
        )))
    }

    pub(crate) fn cycle_pending_username_effect(&mut self, delta: isize) {
        if let Some(pending) = &mut self.pending_username_effect {
            let len = pending.options.len();
            if len > 0 {
                pending.selected =
                    (pending.selected as isize + delta).rem_euclid(len as isize) as usize;
            }
        }
    }

    pub(crate) fn confirm_pending_username_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_username_effect.take()?;
        let effect = pending.selected_effect()?;
        self.service
            .purchase_item_task(self.user_id, pending.sku, None, Some(effect));
        Some(Banner::success(&format!(
            "Activating {}",
            pending.item_name
        )))
    }

    pub(crate) fn cancel_pending_username_effect(&mut self) -> Option<Banner> {
        let pending = self.pending_username_effect.take()?;
        Some(Banner::success(&format!("Cancelled {}", pending.item_name)))
    }

    /// Type one character into the title prompt. The cap is the renderers'
    /// (`TITLE_MAX_LEN`), enforced here so the prompt simply stops accepting
    /// rather than letting someone type a title the purchase would refuse.
    pub(crate) fn push_custom_title_char(&mut self, ch: char) {
        if let Some(pending) = &mut self.pending_custom_title
            && pending.len() < TITLE_MAX_LEN
        {
            pending.input.push(ch);
        }
    }

    pub(crate) fn backspace_custom_title(&mut self) {
        if let Some(pending) = &mut self.pending_custom_title {
            pending.input.pop();
        }
    }

    /// Send the typed title to be screened and bought. A blank prompt is not a
    /// refusal, it is an unfinished one: the modal stays open and nothing is
    /// sent.
    pub(crate) fn confirm_pending_custom_title(&mut self) -> Option<Banner> {
        let text = self.pending_custom_title.as_ref()?.trimmed().to_string();
        if text.is_empty() {
            return Some(Banner::error("Type a title first"));
        }
        let pending = self.pending_custom_title.take()?;
        self.service
            .purchase_custom_title_task(self.user_id, pending.sku, text);
        Some(Banner::success("Screening your title"))
    }

    pub(crate) fn cancel_pending_custom_title(&mut self) -> Option<Banner> {
        self.pending_custom_title.take()?;
        Some(Banner::success("Cancelled custom title"))
    }

    pub(crate) fn adjust_selected_aquarium_fish(&mut self, delta: i32) -> Option<Banner> {
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

    pub(crate) fn use_aquarium_food(&mut self) -> Banner {
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
        if self
            .snapshot
            .active_username_effect
            .is_some_and(|effect| effect.ends_at <= now)
        {
            self.snapshot.active_username_effect = None;
            changed = true;
        }
        // The rentals lapse in the detail pane on their own clock, with no
        // refresh to wait for. `chat_label_*` is deliberately left alone: what
        // the label carries is the label query's call, and it arrives with the
        // next snapshot.
        for rental in [
            &mut self.snapshot.active_badge_rental,
            &mut self.snapshot.active_flag_rental,
            &mut self.snapshot.active_title,
        ] {
            if rental.as_ref().is_some_and(|rental| rental.ends_at <= now) {
                *rental = None;
                changed = true;
            }
        }
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
            pending_username_effect: None,
            pending_custom_title: None,
            category_rects: Cell::new([Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()]),
            item_rects: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
