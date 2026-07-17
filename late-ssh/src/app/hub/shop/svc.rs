use std::{
    collections::{HashMap, HashSet},
    future::poll_fn,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    MutexRecover,
    db::{Db, DbConfig},
    models::{
        chips::{CHIP_USER_CHANGED_CHANNEL, UserChips, listen_for_chip_changes},
        marketplace::{
            AQUARIUM_FISH_ITEM_KIND, AQUARIUM_MAX_FISH, AQUARIUM_SKU, BONSAI_VARIANT_SLOT,
            CHAT_CONSUMABLE_ITEM_KIND, COMPANION_CONSUMABLE_ITEM_KIND, ConsumableUseStatus,
            DYNAMIC_BONSAI_SKU, EquipStatus, FishActiveStatus, MarketplaceItem, PET_COMPANION_SKU,
            PurchaseStatus, SHOP_CATALOG_CHANGED_CHANNEL, SHOP_USER_CHANGED_CHANNEL,
            ULTIMATE_SPELL_KIND, USERNAME_EFFECT_ITEM_KIND, UserPurchase,
            adjust_aquarium_fish_active_by_sku, aquarium_is_hungry, consume_aquarium_food_pinch,
            equip_owned_item_by_sku, list_marketplace_items_for_admin, listen_for_shop_changes,
            purchase_item_by_sku_with_chat_effect, purchase_item_by_sku_with_username_effect,
            unequip_slot, update_marketplace_item_for_admin,
        },
        shop_consumable_effect::ShopConsumableEffect,
        username_effect::{USERNAME_EFFECT_KIND, UsernameEffect},
    },
};
use tokio::sync::{broadcast, watch};
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

use super::catalog::is_chat_badge_slot;
use super::entitlements::ShopEntitlements;
use crate::app::common::username_effect::{NameFlair, NameFlairDirectory};

#[derive(Clone, Debug, Default)]
pub struct ShopSnapshot {
    pub user_id: Option<Uuid>,
    pub balance: i64,
    pub items: Vec<ShopCatalogItem>,
    pub entitlements: ShopEntitlements,
    pub active_room_effects: HashMap<Uuid, Vec<ActiveChatRoomEffect>>,
    pub aquarium_hungry: bool,
    /// The user's live 24h username effect, if any (detail pane shows the
    /// style and remaining time).
    pub active_username_effect: Option<ActiveUsernameEffect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveUsernameEffect {
    pub effect: UsernameEffect,
    pub ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ActiveChatRoomEffect {
    pub effect_kind: String,
    pub source_sku: String,
    pub room_kind: String,
    pub room_visibility: String,
    pub room_permanent: bool,
    pub room_slug: Option<String>,
    pub vibe: Option<String>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ShopCatalogItem {
    pub sku: String,
    pub item_kind: String,
    pub slot: Option<String>,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub owned: bool,
    pub equipped: bool,
    pub quantity: i32,
    pub active_quantity: i32,
    pub remaining_uses: Option<i32>,
    pub badge_emoji: Option<String>,
    pub badge_tier: Option<String>,
    pub aquarium_creature: Option<String>,
    pub aquarium_size: Option<String>,
    pub consumable_category: Option<String>,
    pub effect_kind: Option<String>,
    pub requires_room: bool,
    pub daily_limited: bool,
    /// For `username_effect` items: which style family the item sells
    /// ("glow" | "gradient" | "shimmer"), from the item payload.
    pub username_effect_variant: Option<String>,
}

impl ShopCatalogItem {
    pub fn is_pet_companion(&self) -> bool {
        self.sku == PET_COMPANION_SKU
    }

    pub fn is_dynamic_bonsai(&self) -> bool {
        self.sku == DYNAMIC_BONSAI_SKU
    }

    pub fn is_aquarium(&self) -> bool {
        self.sku == AQUARIUM_SKU
    }

    pub fn is_aquarium_fish(&self) -> bool {
        self.item_kind == AQUARIUM_FISH_ITEM_KIND
    }

    pub fn is_chat_badge(&self) -> bool {
        is_chat_badge_slot(self.slot.as_deref())
    }

    pub fn is_consumable(&self) -> bool {
        matches!(
            self.item_kind.as_str(),
            CHAT_CONSUMABLE_ITEM_KIND | COMPANION_CONSUMABLE_ITEM_KIND
        )
    }

    pub fn is_flag_badge(&self) -> bool {
        self.sku.starts_with("badge_flag_")
    }

    pub fn is_ultimate_spell(&self) -> bool {
        self.item_kind == ULTIMATE_SPELL_KIND
    }

    pub fn is_username_effect(&self) -> bool {
        self.item_kind == USERNAME_EFFECT_ITEM_KIND
    }
}

#[derive(Clone, Debug)]
pub enum ShopEvent {
    ActionCompleted { user_id: Uuid, message: String },
    ActionFailed { user_id: Uuid, message: String },
}

#[derive(Clone)]
pub struct ShopService {
    db: Db,
    snapshot_txs: Arc<Mutex<HashMap<Uuid, watch::Sender<ShopSnapshot>>>>,
    evt_tx: broadcast::Sender<ShopEvent>,
    /// Live username effects, written through on purchase and refreshed from
    /// the `shop_user_changed` notify; sessions resolve it in their tick.
    flair_directory: Option<NameFlairDirectory>,
    /// Announces username-effect purchases to the #lounge ticker.
    activity: Option<crate::app::activity::publisher::ActivityPublisher>,
}

impl ShopService {
    pub fn new(db: Db) -> Self {
        let (evt_tx, _) = broadcast::channel(512);
        Self {
            db,
            snapshot_txs: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
            flair_directory: None,
            activity: None,
        }
    }

    pub fn with_flair_directory(mut self, flair_directory: NameFlairDirectory) -> Self {
        self.flair_directory = Some(flair_directory);
        self
    }

    pub fn with_activity(
        mut self,
        activity: crate::app::activity::publisher::ActivityPublisher,
    ) -> Self {
        self.activity = Some(activity);
        self
    }

    /// Replace the flair directory with the live effect rows. Runs after
    /// every LISTEN registration (startup and reconnects), so effects bought
    /// on other replicas while this listener was down still land here.
    async fn reconcile_flair_directory(&self) -> Result<()> {
        let Some(directory) = &self.flair_directory else {
            return Ok(());
        };
        let entries = self.load_flair_entries().await?;
        crate::app::common::username_effect::set_all(directory, entries);
        Ok(())
    }

    async fn load_flair_entries(&self) -> Result<Vec<(Uuid, NameFlair)>> {
        let client = self.db.get().await?;
        let rows = ShopConsumableEffect::active_user_effects(&client, USERNAME_EFFECT_KIND).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| match UsernameEffect::from_payload(&row.payload) {
                Some(effect) => Some((
                    row.user_id,
                    NameFlair {
                        effect,
                        ends_at: row.ends_at,
                    },
                )),
                None => {
                    tracing::warn!(sku = %row.source_sku, user_id = %row.user_id, "skipping unparseable username effect payload");
                    None
                }
            })
            .collect())
    }

    /// Refresh one user's flair from the DB (LISTEN/NOTIFY path, so effects
    /// bought on another replica land here too).
    async fn refresh_user_flair(&self, user_id: Uuid) -> Result<()> {
        let Some(directory) = &self.flair_directory else {
            return Ok(());
        };
        let client = self.db.get().await?;
        let row = ShopConsumableEffect::active_user_effect_for_user(
            &client,
            user_id,
            USERNAME_EFFECT_KIND,
        )
        .await?;
        let flair = row.and_then(|row| {
            UsernameEffect::from_payload(&row.payload).map(|effect| NameFlair {
                effect,
                ends_at: row.ends_at,
            })
        });
        crate::app::common::username_effect::set_user(directory, user_id, flair);
        Ok(())
    }

    pub fn subscribe_snapshot(&self, user_id: Uuid) -> watch::Receiver<ShopSnapshot> {
        self.snapshot_sender(user_id).subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ShopEvent> {
        self.evt_tx.subscribe()
    }

    fn snapshot_sender(&self, user_id: Uuid) -> watch::Sender<ShopSnapshot> {
        let mut channels = self.snapshot_txs.lock_recover();
        let make = || watch::channel(ShopSnapshot::default()).0;
        let sender = channels.entry(user_id).or_insert_with(&make);
        if sender.is_closed() {
            *sender = make();
        }
        sender.clone()
    }

    fn has_active_snapshot_receiver(&self, user_id: Uuid) -> bool {
        self.snapshot_txs
            .lock_recover()
            .get(&user_id)
            .is_some_and(|sender| sender.receiver_count() > 0)
    }

    fn active_snapshot_users(&self) -> Vec<Uuid> {
        self.snapshot_txs
            .lock_recover()
            .iter()
            .filter_map(|(user_id, sender)| (sender.receiver_count() > 0).then_some(*user_id))
            .collect()
    }

    fn publish_event(&self, event: ShopEvent) {
        let _ = self.evt_tx.send(event);
    }

    pub async fn refresh_user(&self, user_id: Uuid) -> Result<ShopSnapshot> {
        let snapshot = self.load_snapshot(user_id).await?;
        let _ = self.snapshot_sender(user_id).send(snapshot.clone());
        Ok(snapshot)
    }

    async fn refresh_user_if_active(&self, user_id: Uuid) -> Result<()> {
        if self.has_active_snapshot_receiver(user_id) {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    async fn refresh_catalog_for_active_users(&self) -> Result<()> {
        for user_id in self.active_snapshot_users() {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    pub fn refresh_user_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.refresh_user(user_id).await {
                tracing::warn!(error = ?error, user_id = %user_id, "failed to refresh shop snapshot");
            }
        });
    }

    pub fn purchase_item_task(
        &self,
        user_id: Uuid,
        sku: String,
        room_id: Option<Uuid>,
        username_effect: Option<UsernameEffect>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc
                .purchase_item(user_id, &sku, room_id, username_effect)
                .await
            {
                Ok(message) => svc.publish_event(ShopEvent::ActionCompleted { user_id, message }),
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, sku, "shop purchase failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Purchase failed".to_string(),
                    });
                }
            }
        });
    }

    pub fn equip_item_task(&self, user_id: Uuid, sku: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.equip_item(user_id, &sku).await {
                Ok(message) => svc.publish_event(ShopEvent::ActionCompleted { user_id, message }),
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, sku, "shop equip failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not equip item".to_string(),
                    });
                }
            }
        });
    }

    pub fn unequip_slot_task(&self, user_id: Uuid, slot: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.unequip_slot(user_id, &slot).await {
                Ok(message) => svc.publish_event(ShopEvent::ActionCompleted { user_id, message }),
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, slot, "shop unequip failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not clear displayed badge".to_string(),
                    });
                }
            }
        });
    }

    pub fn adjust_aquarium_fish_task(&self, user_id: Uuid, sku: String, delta: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.adjust_aquarium_fish(user_id, &sku, delta).await {
                Ok(message) => svc.publish_event(ShopEvent::ActionCompleted { user_id, message }),
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, sku, delta, "aquarium fish adjust failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not update aquarium".to_string(),
                    });
                }
            }
        });
    }

    pub async fn list_marketplace_items_for_admin(
        &self,
        is_admin: bool,
    ) -> Result<Vec<late_core::models::marketplace::MarketplaceAdminRow>> {
        anyhow::ensure!(is_admin, "admin access required");
        let client = self.db.get().await?;
        list_marketplace_items_for_admin(&client).await
    }

    pub async fn update_marketplace_item_for_admin(
        &self,
        is_admin: bool,
        update: late_core::models::marketplace::MarketplaceAdminUpdate,
    ) -> Result<late_core::models::marketplace::MarketplaceAdminRow> {
        anyhow::ensure!(is_admin, "admin access required");
        let client = self.db.get().await?;
        let row = update_marketplace_item_for_admin(&client, update).await?;
        drop(client);
        self.refresh_catalog_for_active_users().await?;
        Ok(row)
    }

    pub fn use_aquarium_food_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.use_aquarium_food(user_id).await {
                Ok(ConsumableUseStatus::Used) => svc.publish_event(ShopEvent::ActionCompleted {
                    user_id,
                    message: "Fed the aquarium".to_string(),
                }),
                Ok(ConsumableUseStatus::OutOfStock) => svc.publish_event(ShopEvent::ActionFailed {
                    user_id,
                    message: "Buy Aquarium Food first".to_string(),
                }),
                Ok(status) => {
                    tracing::warn!(?status, user_id = %user_id, "aquarium food was not consumed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not feed aquarium".to_string(),
                    });
                }
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, "aquarium food use failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not feed aquarium".to_string(),
                    });
                }
            }
        });
    }

    async fn purchase_item(
        &self,
        user_id: Uuid,
        sku: &str,
        room_id: Option<Uuid>,
        username_effect: Option<UsernameEffect>,
    ) -> Result<String> {
        let mut client = self.db.get().await?;
        let purchase = match username_effect {
            Some(effect) => {
                purchase_item_by_sku_with_username_effect(&mut client, user_id, sku, effect).await?
            }
            None => {
                purchase_item_by_sku_with_chat_effect(&mut client, user_id, sku, room_id).await?
            }
        };

        // A username effect that actually activated goes live immediately for
        // every session on this replica, and its story ships to the ticker.
        // Other replicas catch up from the purchase's shop_user_changed notify.
        if let (Some(effect), Some(row)) = (username_effect, &purchase.username_effect) {
            if let Some(directory) = &self.flair_directory {
                crate::app::common::username_effect::set_user(
                    directory,
                    user_id,
                    Some(NameFlair {
                        effect,
                        ends_at: row.ends_at,
                    }),
                );
            }
            if let Some(activity) = &self.activity {
                activity.username_effect_task(user_id, effect);
            }
        }

        let message = match &purchase.purchase {
            None => "Item is not available".to_string(),
            Some(result) => match result.status {
                PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded
                    if result.item.item_kind == USERNAME_EFFECT_ITEM_KIND =>
                {
                    format!("Activated {} (24h)", result.item.name)
                }
                PurchaseStatus::Purchased if result.item.item_kind == AQUARIUM_FISH_ITEM_KIND => {
                    format!("Bought {} (owned {})", result.item.name, result.quantity)
                }
                PurchaseStatus::Purchased if result.item.item_kind == CHAT_CONSUMABLE_ITEM_KIND => {
                    format!("Activated {}", result.item.name)
                }
                PurchaseStatus::Purchased if is_consumable_kind(&result.item.item_kind) => {
                    format!("Bought {}", result.item.name)
                }
                PurchaseStatus::Purchased => format!("Unlocked {}", result.item.name),
                PurchaseStatus::QuantityAdded
                    if result.item.item_kind == CHAT_CONSUMABLE_ITEM_KIND =>
                {
                    format!("Activated {}", result.item.name)
                }
                PurchaseStatus::QuantityAdded if is_consumable_kind(&result.item.item_kind) => {
                    format!("Bought {} ({} total)", result.item.name, result.quantity)
                }
                PurchaseStatus::QuantityAdded => {
                    format!("Bought {} (owned {})", result.item.name, result.quantity)
                }
                PurchaseStatus::AlreadyOwned => format!("{} already unlocked", result.item.name),
                PurchaseStatus::InsufficientFunds => {
                    format!(
                        "Need {} chips for {}",
                        result.item.price_chips, result.item.name
                    )
                }
                PurchaseStatus::RequiresAquarium => "Unlock Aquarium first".to_string(),
                PurchaseStatus::DailyLimitReached => {
                    format!("{} is limited to once per day", result.item.name)
                }
            },
        };

        drop(client);
        if purchase.refresh_all_active_users {
            self.refresh_catalog_for_active_users().await?;
        } else {
            self.refresh_user(user_id).await?;
        }
        Ok(message)
    }

    async fn adjust_aquarium_fish(&self, user_id: Uuid, sku: &str, delta: i32) -> Result<String> {
        let mut client = self.db.get().await?;
        let result = adjust_aquarium_fish_active_by_sku(&mut client, user_id, sku, delta).await?;
        drop(client);

        let message = match result {
            None => "Fish is not available".to_string(),
            Some(result) => match result.status {
                FishActiveStatus::Changed => {
                    format!(
                        "{} active {}/{}",
                        result.item.name, result.active_quantity, result.quantity
                    )
                }
                FishActiveStatus::NotOwned => format!("Buy {} first", result.item.name),
                FishActiveStatus::NotFish => "That item is not a fish".to_string(),
                FishActiveStatus::AtZero => format!("No active {} to remove", result.item.name),
                FishActiveStatus::AtOwnedQuantity => {
                    format!("All owned {} are active", result.item.name)
                }
                FishActiveStatus::TankFull => {
                    format!("Aquarium has {AQUARIUM_MAX_FISH} active fish")
                }
            },
        };

        self.refresh_user(user_id).await?;
        Ok(message)
    }

    async fn use_aquarium_food(&self, user_id: Uuid) -> Result<ConsumableUseStatus> {
        let mut client = self.db.get().await?;
        let result = consume_aquarium_food_pinch(&mut client, user_id).await?;
        drop(client);
        self.refresh_user(user_id).await?;
        Ok(result.status)
    }

    async fn equip_item(&self, user_id: Uuid, sku: &str) -> Result<String> {
        let mut client = self.db.get().await?;
        let result = equip_owned_item_by_sku(&mut client, user_id, sku).await?;
        drop(client);

        let message = match result {
            None => "Item is not available".to_string(),
            Some(result) => match result.status {
                EquipStatus::Equipped if result.item.sku == DYNAMIC_BONSAI_SKU => {
                    "Using Dynamic Bonsai".to_string()
                }
                EquipStatus::Equipped => format!("Displaying {}", result.item.name),
                EquipStatus::AlreadyEquipped if result.item.sku == DYNAMIC_BONSAI_SKU => {
                    "Dynamic Bonsai already active".to_string()
                }
                EquipStatus::AlreadyEquipped => format!("{} already displayed", result.item.name),
                EquipStatus::NotOwned => format!("You do not own {}", result.item.name),
                EquipStatus::NotEquippable => format!("{} cannot be displayed", result.item.name),
            },
        };

        self.refresh_user(user_id).await?;
        Ok(message)
    }

    async fn unequip_slot(&self, user_id: Uuid, slot: &str) -> Result<String> {
        let mut client = self.db.get().await?;
        let changed = unequip_slot(&mut client, user_id, slot).await?;
        drop(client);

        self.refresh_user(user_id).await?;
        if changed {
            if slot == BONSAI_VARIANT_SLOT {
                Ok("Using classic Bonsai".to_string())
            } else {
                Ok("Cleared displayed badge".to_string())
            }
        } else if slot == BONSAI_VARIANT_SLOT {
            Ok("Classic Bonsai already active".to_string())
        } else {
            Ok("No badge is displayed".to_string())
        }
    }

    async fn load_snapshot(&self, user_id: Uuid) -> Result<ShopSnapshot> {
        let client = self.db.get().await?;
        let chips = UserChips::ensure(&client, user_id).await?;
        let items = MarketplaceItem::list_visible(&client).await?;
        let purchases = UserPurchase::list_for_user(&client, user_id).await?;
        let mut active_room_effects: HashMap<Uuid, Vec<ActiveChatRoomEffect>> = HashMap::new();
        let active_effect_rows = ShopConsumableEffect::active_room_effects(&client).await?;
        let active_effect_room_ids = active_effect_rows
            .iter()
            .filter_map(|effect| effect.room_id)
            .collect::<Vec<_>>();
        let mut active_effect_room_meta = HashMap::new();
        if !active_effect_room_ids.is_empty() {
            let rows = client
                .query(
                    "SELECT id, kind, visibility, permanent, slug
                     FROM chat_rooms
                     WHERE id = ANY($1)",
                    &[&active_effect_room_ids],
                )
                .await?;
            for row in rows {
                active_effect_room_meta.insert(
                    row.get::<_, Uuid>("id"),
                    (
                        row.get::<_, String>("kind"),
                        row.get::<_, String>("visibility"),
                        row.get::<_, bool>("permanent"),
                        row.get::<_, Option<String>>("slug"),
                    ),
                );
            }
        }
        for effect in active_effect_rows {
            let Some(room_id) = effect.room_id else {
                continue;
            };
            let Some((room_kind, room_visibility, room_permanent, room_slug)) =
                active_effect_room_meta.get(&room_id).cloned()
            else {
                continue;
            };
            active_room_effects
                .entry(room_id)
                .or_default()
                .push(ActiveChatRoomEffect {
                    effect_kind: effect.effect_kind,
                    source_sku: effect.source_sku,
                    room_kind,
                    room_visibility,
                    room_permanent,
                    room_slug,
                    vibe: effect
                        .payload
                        .get("vibe")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    ends_at: effect.ends_at,
                });
        }
        let aquarium_hungry = aquarium_is_hungry(&client, user_id).await?;
        let active_username_effect = ShopConsumableEffect::active_user_effect_for_user(
            &client,
            user_id,
            USERNAME_EFFECT_KIND,
        )
        .await?
        .and_then(|row| {
            UsernameEffect::from_payload(&row.payload).map(|effect| ActiveUsernameEffect {
                effect,
                ends_at: row.ends_at,
            })
        });

        let mut purchases_by_item = HashMap::with_capacity(purchases.len());
        for purchase in purchases {
            purchases_by_item.insert(purchase.item_id, purchase);
        }

        let mut owned_skus = HashSet::new();
        let catalog = items
            .into_iter()
            .map(|item| {
                let purchase = purchases_by_item.get(&item.id);
                let item_kind = item.item_kind.clone();
                let owned = purchase.is_some_and(|purchase| {
                    !is_consumable_kind(&item_kind) || purchase.quantity > 0
                });
                if owned {
                    owned_skus.insert(item.sku.clone());
                }
                let equipped = match (
                    purchase.and_then(|purchase| purchase.equipped_slot.as_deref()),
                    item.slot.as_deref(),
                ) {
                    (Some(equipped_slot), Some(item_slot)) => equipped_slot == item_slot,
                    _ => false,
                };
                let badge_emoji = item
                    .payload
                    .get("emoji")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let badge_tier = item
                    .payload
                    .get("tier")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let aquarium_creature = item
                    .payload
                    .get("creature")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let aquarium_size = item
                    .payload
                    .get("size")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let consumable_category = item
                    .payload
                    .get("category")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let effect_kind = item
                    .payload
                    .get("effect_kind")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                let requires_room =
                    item.payload.get("target").and_then(|value| value.as_str()) == Some("room");
                let daily_limited = item
                    .payload
                    .get("daily_limit")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let username_effect_variant = (item_kind == USERNAME_EFFECT_ITEM_KIND)
                    .then(|| {
                        item.payload
                            .get("variant")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned)
                    })
                    .flatten();
                ShopCatalogItem {
                    sku: item.sku,
                    item_kind,
                    slot: item.slot,
                    name: item.name,
                    description: item.description,
                    price_chips: item.price_chips,
                    owned,
                    quantity: purchase.map(|purchase| purchase.quantity).unwrap_or(0),
                    active_quantity: purchase
                        .map(|purchase| purchase.active_quantity)
                        .unwrap_or(0),
                    remaining_uses: purchase.and_then(|purchase| purchase.remaining_uses),
                    equipped,
                    badge_emoji,
                    badge_tier,
                    aquarium_creature,
                    aquarium_size,
                    consumable_category,
                    effect_kind,
                    requires_room,
                    daily_limited,
                    username_effect_variant,
                }
            })
            .collect();

        Ok(ShopSnapshot {
            user_id: Some(user_id),
            balance: chips.balance,
            items: catalog,
            entitlements: ShopEntitlements::from_owned_skus(owned_skus),
            active_room_effects,
            aquarium_hungry,
            active_username_effect,
        })
    }

    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = svc.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "shop postgres listener stopped");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        })
    }

    async fn listen_once(&self, db_config: &DbConfig) -> Result<()> {
        let mut config = tokio_postgres::Config::new();
        config.host(&db_config.host);
        config.port(db_config.port);
        config.user(&db_config.user);
        config.password(&db_config.password);
        config.dbname(&db_config.dbname);

        let (client, mut connection) = config.connect(NoTls).await?;
        let listen = async {
            listen_for_shop_changes(&client).await?;
            listen_for_chip_changes(&client).await
        };
        tokio::pin!(listen);
        loop {
            tokio::select! {
                result = &mut listen => {
                    result?;
                    break;
                }
                message = poll_fn(|cx| connection.poll_message(cx)) => {
                    let Some(message) = message else {
                        return Ok(());
                    };
                    self.handle_async_message(message?).await?;
                }
            }
        }

        // LISTEN is registered; notifications now buffer on the connection,
        // so a full snapshot here cannot race a concurrent purchase. On error
        // the caller reconnects and reconciles again.
        self.reconcile_flair_directory().await?;

        loop {
            let Some(message) = poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };

            self.handle_async_message(message?).await?;
        }
    }

    async fn handle_async_message(&self, message: AsyncMessage) -> Result<()> {
        match message {
            AsyncMessage::Notification(notification) => match notification.channel() {
                SHOP_USER_CHANGED_CHANNEL => {
                    if let Ok(user_id) = notification.payload().parse::<Uuid>() {
                        // Flair refreshes unconditionally: an effect is
                        // visible to every session, not only shop viewers.
                        // Chip notifies stay out of this path on purpose;
                        // they fire far too often for a per-notify query.
                        // Errors propagate so the listener reconnects and
                        // reconciles instead of dropping the update.
                        self.refresh_user_flair(user_id).await?;
                        self.refresh_user_if_active(user_id).await?;
                    }
                }
                CHIP_USER_CHANGED_CHANNEL => {
                    if let Ok(user_id) = notification.payload().parse::<Uuid>() {
                        self.refresh_user_if_active(user_id).await?;
                    }
                }
                SHOP_CATALOG_CHANGED_CHANNEL => {
                    self.refresh_catalog_for_active_users().await?;
                }
                _ => {}
            },
            AsyncMessage::Notice(notice) => {
                tracing::debug!(notice = ?notice, "postgres shop listener notice");
            }
            _ => {}
        }
        Ok(())
    }
}

fn is_consumable_kind(item_kind: &str) -> bool {
    matches!(
        item_kind,
        CHAT_CONSUMABLE_ITEM_KIND | COMPANION_CONSUMABLE_ITEM_KIND
    )
}
