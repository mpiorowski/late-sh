use std::{
    collections::{HashMap, HashSet},
    future::poll_fn,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    MutexRecover,
    db::{Db, DbConfig},
    models::{
        bonsai_decay_protection::BonsaiDecayProtection,
        chat_room::ChatRoom,
        chips::{CHIP_USER_CHANGED_CHANNEL, UserChips, listen_for_chip_changes},
        marketplace::{
            AQUARIUM_FISH_ITEM_KIND, AQUARIUM_MAX_FISH, AQUARIUM_SKU, BONSAI_CONSUMABLE_ITEM_KIND,
            BONSAI_DECAY_SHIELD_SKU, BONSAI_VARIANT_SLOT, CHAT_BADGE_SLOT,
            CHAT_CONSUMABLE_ITEM_KIND, CHAT_FLAG_SLOT, COMPANION_CONSUMABLE_ITEM_KIND,
            ConsumableUseStatus, DYNAMIC_BONSAI_SKU, EquipStatus, FishActiveStatus,
            MarketplaceItem, PET_COMPANION_SKU, PurchaseResult, PurchaseStatus,
            PurchaseWithEffectResult, SHOP_CATALOG_CHANGED_CHANNEL, SHOP_USER_CHANGED_CHANNEL,
            ULTIMATE_SPELL_KIND, USERNAME_EFFECT_ITEM_KIND, UserPurchase,
            adjust_aquarium_fish_active_by_sku, aquarium_is_hungry, consume_aquarium_food_pinch,
            equip_owned_item_by_sku, listen_for_shop_changes,
            purchase_item_by_sku_with_chat_effect, purchase_item_by_sku_with_custom_title,
            purchase_item_by_sku_with_username_effect, rental_duration_secs, unequip_slot,
        },
        milestone::{MILESTONE_BADGE_ITEM_KIND, MilestoneBadge},
        rental::{
            BADGE_RENTAL_ITEM_KIND, BadgeRental, CustomTitle, RENTAL_DAY_SECS, TITLE_EFFECT_KIND,
            TITLE_RENTAL_ITEM_KIND, duration_tag, is_custom_title, title_from_payload,
        },
        shop_consumable_effect::ShopConsumableEffect,
        user::User,
        username_effect::{USERNAME_EFFECT_KIND, UsernameEffect},
    },
};
use tokio::sync::{broadcast, watch};
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

use super::entitlements::ShopEntitlements;
use crate::app::ai::screen::{TitleScreen, screen_custom_title};
use crate::app::ai::svc::AiService;
use crate::app::common::username_effect::{FlairEffect, FlairTitle, NameFlair, NameFlairDirectory};

#[derive(Clone, Debug, Default)]
pub struct ShopSnapshot {
    pub user_id: Option<Uuid>,
    pub balance: i64,
    pub items: Vec<ShopCatalogItem>,
    pub entitlements: ShopEntitlements,
    pub active_room_effects: HashMap<Uuid, Vec<ActiveChatRoomEffect>>,
    pub aquarium_hungry: bool,
    /// The user's live username effect, if any (detail pane shows the style
    /// and remaining time).
    pub active_username_effect: Option<ActiveUsernameEffect>,
    /// The user's live Bonsai Decay Shield window, if any (detail pane shows
    /// the remaining time).
    pub active_bonsai_decay_protection: Option<BonsaiDecayProtection>,
    /// The user's live chat badge rental, flag rental, and title, if any.
    /// The detail panes show what is running and how long is left.
    pub active_badge_rental: Option<ActiveRental>,
    pub active_flag_rental: Option<ActiveRental>,
    pub active_title: Option<ActiveRental>,
    /// What this user's chat label actually shows, straight from the one
    /// query that decides it for every viewer
    /// (`User::list_chat_author_metadata`). The buyer's own session paints
    /// exactly what everyone else will, with no second copy of the rule.
    pub chat_label_badge: Option<String>,
    pub chat_label_flag: Option<String>,
    /// Whether the Shop can sell a title the buyer writes themselves. Custom
    /// text is screened before the purchase transaction opens, so with no AI
    /// configured the custom SKUs render as unavailable rather than shipping
    /// unscreened.
    pub custom_titles_available: bool,
}

/// One live user-scoped rental as the Shop shows it: what it is, which SKU
/// bought it, and when it lapses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveRental {
    /// The badge emoji or the title text, whichever this rental sells.
    pub label: String,
    pub source_sku: String,
    pub ends_at: DateTime<Utc>,
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
    /// For every rental kind (username effect, badge/flag rental, title): how
    /// long the bought window runs, read the same way the purchase
    /// transaction reads it, so the shop never quotes a window the activation
    /// would not honour.
    pub rental_duration_secs: Option<i64>,
    /// Which chat-label slot this item fills, read from the rental payload.
    /// Only a rental fills one: the Badges and Flags tabs are rentals top to
    /// bottom, and a rental never touches `equipped_slot`.
    pub badge_slot: Option<String>,
    /// Whether this title rental sells a text the buyer writes rather than one
    /// the catalog carries.
    pub custom_title: bool,
}

impl ShopCatalogItem {
    pub fn is_pet_companion(&self) -> bool {
        self.sku == PET_COMPANION_SKU
    }

    pub fn is_dynamic_bonsai(&self) -> bool {
        self.sku == DYNAMIC_BONSAI_SKU
    }

    pub fn is_bonsai_decay_shield(&self) -> bool {
        self.sku == BONSAI_DECAY_SHIELD_SKU
    }

    pub fn is_aquarium(&self) -> bool {
        self.sku == AQUARIUM_SKU
    }

    pub fn is_aquarium_fish(&self) -> bool {
        self.item_kind == AQUARIUM_FISH_ITEM_KIND
    }

    pub fn is_chat_badge(&self) -> bool {
        self.badge_slot.is_some()
    }

    pub fn is_badge_rental(&self) -> bool {
        self.item_kind == BADGE_RENTAL_ITEM_KIND
    }

    pub fn is_title_rental(&self) -> bool {
        self.item_kind == TITLE_RENTAL_ITEM_KIND
    }

    /// A title rental the buyer writes the text for. Enter on one opens a
    /// prompt rather than buying outright.
    pub fn is_custom_title(&self) -> bool {
        self.custom_title
    }

    /// Every timed item: the shop quotes a window and a rebuy resets it.
    pub fn is_rental(&self) -> bool {
        self.is_username_effect() || self.is_badge_rental() || self.is_title_rental()
    }

    pub fn is_consumable(&self) -> bool {
        matches!(
            self.item_kind.as_str(),
            CHAT_CONSUMABLE_ITEM_KIND
                | COMPANION_CONSUMABLE_ITEM_KIND
                | BONSAI_CONSUMABLE_ITEM_KIND
        )
    }

    pub fn is_flag_badge(&self) -> bool {
        self.badge_slot.as_deref() == Some(CHAT_FLAG_SLOT)
    }

    pub fn is_ultimate_spell(&self) -> bool {
        self.item_kind == ULTIMATE_SPELL_KIND
    }

    /// A burn milestone: permanent, never equipped, and the dearest one owned
    /// is the one that shows. Shares the Ultimates tab with the spells, so
    /// the tab has to tell them apart before offering to cast anything.
    pub fn is_milestone_badge(&self) -> bool {
        self.item_kind == MILESTONE_BADGE_ITEM_KIND
    }

    pub fn is_username_effect(&self) -> bool {
        self.item_kind == USERNAME_EFFECT_ITEM_KIND
    }

    /// How long this item's rental runs, for callers that already know the
    /// item is one. The catalog fills the duration for every rental, so the
    /// day-tier fallback here is only a floor for a malformed payload, never a
    /// second policy.
    pub fn rental_duration(&self) -> i64 {
        self.rental_duration_secs.unwrap_or(RENTAL_DAY_SECS)
    }
}

/// The #lounge story a purchase ships, if any. Read off what actually
/// activated in the transaction, never off what was asked for: a refused or
/// uncharged purchase announces nothing.
enum PurchaseStory {
    UsernameEffect {
        effect: UsernameEffect,
        duration: i64,
    },
    BadgeRented {
        emoji: String,
        duration: i64,
    },
    TitleApplied {
        title: String,
        duration: i64,
    },
    /// A burn milestone was unlocked. The line names the price, because the
    /// price is the whole product: the badge is the receipt.
    BurnMilestone {
        name: String,
        emoji: String,
        price: i64,
    },
}

fn purchase_story(
    purchase: &PurchaseWithEffectResult,
    result: &PurchaseResult,
) -> Option<PurchaseStory> {
    match result.status {
        PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded => {}
        PurchaseStatus::AlreadyOwned
        | PurchaseStatus::InsufficientFunds
        | PurchaseStatus::RequiresAquarium
        | PurchaseStatus::DailyLimitReached => return None,
    }
    let duration = rental_duration_secs(&result.item);
    match result.item.item_kind.as_str() {
        USERNAME_EFFECT_ITEM_KIND => {
            let row = purchase.username_effect.as_ref()?;
            UsernameEffect::from_payload(&row.payload)
                .map(|effect| PurchaseStory::UsernameEffect { effect, duration })
        }
        BADGE_RENTAL_ITEM_KIND => {
            let row = purchase.badge_rental.as_ref()?;
            BadgeRental::from_payload(&row.payload).map(|rental| PurchaseStory::BadgeRented {
                emoji: rental.emoji,
                duration,
            })
        }
        TITLE_RENTAL_ITEM_KIND => {
            let row = purchase.title_rental.as_ref()?;
            title_from_payload(&row.payload)
                .map(|title| PurchaseStory::TitleApplied { title, duration })
        }
        MILESTONE_BADGE_ITEM_KIND => {
            // Read off the item that actually charged, not off a payload the
            // transaction wrote: a milestone activates nothing, the purchase
            // row is the whole event.
            late_core::models::milestone::emoji_from_payload(&result.item.payload).map(|emoji| {
                PurchaseStory::BurnMilestone {
                    name: result.item.name.clone(),
                    emoji,
                    price: result.item.price_chips,
                }
            })
        }
        _ => None,
    }
}

/// What renting a buyer-written title came to. `Refused` carries the banner
/// the buyer sees and means no chips moved.
#[derive(Debug, PartialEq, Eq)]
enum CustomTitleOutcome {
    Rented(String),
    Refused(String),
}

/// What a purchase transaction came back with, once the rest of the app has
/// been told: the status the ledger decided on (`None` when the SKU was not
/// on sale) and the banner copy for it.
#[derive(Debug, PartialEq, Eq)]
struct SettledPurchase {
    status: Option<PurchaseStatus>,
    message: String,
}

/// Only a transaction that actually charged is a rental. The precheck runs
/// before a screen that can take 30s and the transaction re-checks the
/// balance, so a refusal from the ledger is real and lands on the failure
/// banner like every other refusal in the flow.
fn custom_title_outcome(settled: SettledPurchase) -> CustomTitleOutcome {
    match settled.status {
        Some(PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded) => {
            CustomTitleOutcome::Rented(settled.message)
        }
        Some(
            PurchaseStatus::AlreadyOwned
            | PurchaseStatus::InsufficientFunds
            | PurchaseStatus::RequiresAquarium
            | PurchaseStatus::DailyLimitReached,
        )
        | None => CustomTitleOutcome::Refused(settled.message),
    }
}

/// Shown whenever no verdict on a title can be had: AI switched off, or the
/// screen came back with nothing. The Shop hides the custom SKUs in that case,
/// so this is the race where a session's snapshot is a beat behind.
const CUSTOM_TITLES_UNAVAILABLE: &str = "Custom titles are closed right now";

/// The least time between two screens for one user. Every screen is a paid
/// API call and a refused one is free to the buyer by design, so without this
/// a held-down Enter is an unmetered bill.
const CUSTOM_TITLE_SCREEN_COOLDOWN: Duration = Duration::from_secs(10);

/// Why a custom title is refused before the screen is even asked. Both gates
/// exist to keep a paid API call from being made for a purchase that could
/// not complete or that was just made: the buyer cannot afford the tier (the
/// same `balance < price` rule the purchase transaction applies), or their
/// last screen was inside the cooldown. `None` means go ahead and screen.
fn custom_title_precheck(
    balance: i64,
    price_chips: i64,
    item_name: &str,
    last_screen: Option<Instant>,
    now: Instant,
) -> Option<String> {
    if balance < price_chips {
        return Some(format!("Need {price_chips} chips for {item_name}"));
    }
    match last_screen {
        Some(last) if now.duration_since(last) < CUSTOM_TITLE_SCREEN_COOLDOWN => {
            let wait = CUSTOM_TITLE_SCREEN_COOLDOWN - now.duration_since(last);
            Some(format!(
                "Wait {}s before screening another title",
                wait.as_secs().max(1)
            ))
        }
        Some(_) | None => None,
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
    /// Screens buyer-written titles before they are charged for. Absent (or
    /// switched off) means the Shop sells no custom titles at all.
    ai_service: Option<AiService>,
    /// When each user last had a title screened, for
    /// `CUSTOM_TITLE_SCREEN_COOLDOWN`. Process-local like the chat gift
    /// cooldown: a session lives on one replica, and this meters API spend,
    /// not game state.
    screen_cooldowns: Arc<Mutex<HashMap<Uuid, Instant>>>,
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
            ai_service: None,
            screen_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_ai_service(mut self, ai_service: AiService) -> Self {
        self.ai_service = Some(ai_service);
        self
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
        let effect_rows =
            ShopConsumableEffect::active_user_effects(&client, USERNAME_EFFECT_KIND).await?;
        let title_rows =
            ShopConsumableEffect::active_user_effects(&client, TITLE_EFFECT_KIND).await?;

        let milestone_rows = MilestoneBadge::highest_for_all(&client).await?;

        let mut entries: HashMap<Uuid, NameFlair> = HashMap::new();
        for (user_id, emoji) in milestone_rows {
            entries.entry(user_id).or_default().milestone = Some(emoji);
        }
        for row in effect_rows {
            match UsernameEffect::from_payload(&row.payload) {
                Some(effect) => {
                    entries.entry(row.user_id).or_default().effect = Some(FlairEffect {
                        effect,
                        ends_at: row.ends_at,
                    });
                }
                None => {
                    tracing::warn!(sku = %row.source_sku, user_id = %row.user_id, "skipping unparseable username effect payload");
                }
            }
        }
        for row in title_rows {
            match title_from_payload(&row.payload) {
                Some(text) => {
                    entries.entry(row.user_id).or_default().title = Some(FlairTitle {
                        text,
                        ends_at: row.ends_at,
                    });
                }
                None => {
                    tracing::warn!(sku = %row.source_sku, user_id = %row.user_id, "skipping unparseable title payload");
                }
            }
        }
        Ok(entries.into_iter().collect())
    }

    /// Refresh one user's flair from the DB (LISTEN/NOTIFY path, so effects
    /// bought on another replica land here too). Both halves are read in one
    /// query and written as one entry, so refreshing after a title purchase
    /// never drops a live color effect and vice versa.
    async fn refresh_user_flair(&self, user_id: Uuid) -> Result<()> {
        let Some(directory) = &self.flair_directory else {
            return Ok(());
        };
        let client = self.db.get().await?;
        let rows = ShopConsumableEffect::active_user_effects_for_user(
            &client,
            user_id,
            &[USERNAME_EFFECT_KIND, TITLE_EFFECT_KIND],
        )
        .await?;
        // The milestone is a purchase, not an effect row, so it is read on
        // its own here. It has to be read on this path at all because the
        // whole entry is replaced below: leaving it out would drop a
        // 500,000-chip glyph the moment its owner rented a badge.
        let mut flair = NameFlair {
            milestone: MilestoneBadge::highest_for_user(&client, user_id).await?,
            ..NameFlair::default()
        };
        for row in rows {
            // The query orders `ends_at DESC` inside each kind, so the first
            // row of a kind is the live one; a stray older row never wins.
            match row.effect_kind.as_str() {
                USERNAME_EFFECT_KIND if flair.effect.is_none() => {
                    flair.effect =
                        UsernameEffect::from_payload(&row.payload).map(|effect| FlairEffect {
                            effect,
                            ends_at: row.ends_at,
                        });
                }
                TITLE_EFFECT_KIND if flair.title.is_none() => {
                    flair.title = title_from_payload(&row.payload).map(|text| FlairTitle {
                        text,
                        ends_at: row.ends_at,
                    });
                }
                _ => {}
            }
        }
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

    /// Rent a title the buyer wrote. Every outcome the flow has is listed
    /// here, once: the text was refused before any money moved, the screen had
    /// no verdict to give, the call itself broke, or the purchase went
    /// through. A refusal is an `ActionFailed` banner and nothing else, which
    /// is the whole "never charge for a no-op" rule.
    pub fn purchase_custom_title_task(&self, user_id: Uuid, sku: String, text: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.purchase_custom_title(user_id, &sku, &text).await {
                Ok(CustomTitleOutcome::Rented(message)) => {
                    svc.publish_event(ShopEvent::ActionCompleted { user_id, message })
                }
                Ok(CustomTitleOutcome::Refused(message)) => {
                    tracing::info!(user_id = %user_id, sku, reason = %message, "custom title refused");
                    svc.publish_event(ShopEvent::ActionFailed { user_id, message })
                }
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, sku, "custom title purchase failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Could not check that title, nothing was charged".to_string(),
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
        drop(client);

        Ok(self.settle_purchase(user_id, purchase).await?.message)
    }

    /// Rent a title the buyer wrote themselves: validate, screen, then buy.
    /// The order matters. Nothing touches the chip ledger until the text has
    /// passed both gates, so every refusal below is free.
    async fn purchase_custom_title(
        &self,
        user_id: Uuid,
        sku: &str,
        text: &str,
    ) -> Result<CustomTitleOutcome> {
        let title = match CustomTitle::parse(text) {
            Ok(title) => title,
            Err(error) => return Ok(CustomTitleOutcome::Refused(error.message().to_string())),
        };
        let Some(ai_service) = self.ai_service.as_ref() else {
            return Ok(CustomTitleOutcome::Refused(
                CUSTOM_TITLES_UNAVAILABLE.to_string(),
            ));
        };

        // The screen is a paid call. Before making one: the SKU must be a
        // custom title that is on sale, the buyer must be able to afford it
        // under the purchase's own rule, and their last screen must be
        // outside the cooldown. Every refusal here is free and makes no call.
        let (item_name, price_chips, balance) = {
            let client = self.db.get().await?;
            let item = match MarketplaceItem::find_visible_by_sku(&client, sku).await? {
                Some(item)
                    if item.item_kind == TITLE_RENTAL_ITEM_KIND
                        && is_custom_title(&item.payload) =>
                {
                    item
                }
                Some(_) | None => {
                    return Ok(CustomTitleOutcome::Refused(
                        "That item does not take a custom title".to_string(),
                    ));
                }
            };
            let chips = UserChips::ensure(&client, user_id).await?;
            (item.name, item.price_chips, chips.balance)
        };
        let now = Instant::now();
        {
            let mut cooldowns = self.screen_cooldowns.lock_recover();
            let last_screen = cooldowns.get(&user_id).copied();
            if let Some(message) =
                custom_title_precheck(balance, price_chips, &item_name, last_screen, now)
            {
                return Ok(CustomTitleOutcome::Refused(message));
            }
            cooldowns.insert(user_id, now);
        }

        match screen_custom_title(ai_service, title.as_str()).await? {
            TitleScreen::Allowed => {}
            TitleScreen::Refused { reason } => return Ok(CustomTitleOutcome::Refused(reason)),
            TitleScreen::Unavailable => {
                return Ok(CustomTitleOutcome::Refused(
                    CUSTOM_TITLES_UNAVAILABLE.to_string(),
                ));
            }
        }

        tracing::info!(user_id = %user_id, sku, "custom title passed the screen");
        let mut client = self.db.get().await?;
        let purchase =
            purchase_item_by_sku_with_custom_title(&mut client, user_id, sku, title).await?;
        drop(client);
        let settled = self.settle_purchase(user_id, purchase).await?;
        Ok(custom_title_outcome(settled))
    }

    /// Everything a completed purchase transaction owes the rest of the app:
    /// the flair write-through, the #lounge story, the banner copy, and the
    /// snapshot refresh. Shared by every purchase path so a new one cannot
    /// quietly skip one of them.
    async fn settle_purchase(
        &self,
        user_id: Uuid,
        purchase: PurchaseWithEffectResult,
    ) -> Result<SettledPurchase> {
        // Flair that actually activated goes live immediately for every
        // session on this replica: re-read both halves rather than writing one
        // through, so a title purchase never drops a live color effect (and
        // vice versa). Other replicas catch up from the purchase's
        // shop_user_changed notify, which lands on the same code path.
        let flair_changed = purchase.username_effect.is_some()
            || purchase.title_rental.is_some()
            || purchase.purchase.as_ref().is_some_and(|result| {
                matches!(result.status, PurchaseStatus::Purchased)
                    && result.item.item_kind == MILESTONE_BADGE_ITEM_KIND
            });

        // The stories that ship to the #lounge ticker. Each names what other
        // people will now see next to this player's name.
        if let Some(result) = &purchase.purchase {
            match (&self.activity, purchase_story(&purchase, result)) {
                (Some(activity), Some(PurchaseStory::UsernameEffect { effect, duration })) => {
                    activity.username_effect_task(user_id, effect, duration);
                }
                (Some(activity), Some(PurchaseStory::BadgeRented { emoji, duration })) => {
                    activity.badge_rented_task(user_id, emoji, duration);
                }
                (Some(activity), Some(PurchaseStory::TitleApplied { title, duration })) => {
                    activity.title_applied_task(user_id, title, duration);
                }
                (Some(activity), Some(PurchaseStory::BurnMilestone { name, emoji, price })) => {
                    activity.burn_milestone_task(user_id, name, emoji, price);
                }
                (None, _) | (_, None) => {}
            }
        }

        let status = purchase.purchase.as_ref().map(|result| result.status);
        let message = match &purchase.purchase {
            None => "Item is not available".to_string(),
            Some(result) => match result.status {
                PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded
                    if result.item.item_kind == USERNAME_EFFECT_ITEM_KIND =>
                {
                    format!(
                        "Activated {} ({})",
                        result.item.name,
                        duration_tag(rental_duration_secs(&result.item))
                    )
                }
                PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded
                    if result.item.item_kind == BADGE_RENTAL_ITEM_KIND =>
                {
                    format!(
                        "Rented {} ({})",
                        result.item.name,
                        duration_tag(rental_duration_secs(&result.item))
                    )
                }
                PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded
                    if result.item.item_kind == TITLE_RENTAL_ITEM_KIND =>
                {
                    // The activated row, not the SKU name: a custom title's
                    // name is "Custom Title", and the buyer wants to read back
                    // the words they typed.
                    let worn = purchase
                        .title_rental
                        .as_ref()
                        .and_then(|row| title_from_payload(&row.payload))
                        .unwrap_or_else(|| result.item.name.clone());
                    format!(
                        "Wearing \"{}\" ({})",
                        worn,
                        duration_tag(rental_duration_secs(&result.item))
                    )
                }
                PurchaseStatus::Purchased | PurchaseStatus::QuantityAdded
                    if result.item.item_kind == BONSAI_CONSUMABLE_ITEM_KIND =>
                {
                    match &purchase.bonsai_decay_protection {
                        Some(effect) => format!(
                            "Bonsai protected until {} (UTC)",
                            effect.ends_at.date_naive()
                        ),
                        None => format!("Bought {}", result.item.name),
                    }
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

        if flair_changed {
            self.refresh_user_flair(user_id).await?;
        }
        if purchase.refresh_all_active_users {
            self.refresh_catalog_for_active_users().await?;
        } else {
            self.refresh_user(user_id).await?;
        }
        Ok(SettledPurchase { status, message })
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
        for room in ChatRoom::list_by_ids(&client, &active_effect_room_ids).await? {
            active_effect_room_meta.insert(
                room.id,
                (room.kind, room.visibility, room.permanent, room.slug),
            );
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
                    ends_at: effect.ends_at,
                });
        }
        let aquarium_hungry = aquarium_is_hungry(&client, user_id).await?;

        // One query for every user-scoped rental the Shop shows. Rows arrive
        // ordered `ends_at DESC` inside each kind, so the first row of a kind
        // is the live one.
        let rental_rows = ShopConsumableEffect::active_user_effects_for_user(
            &client,
            user_id,
            &[
                USERNAME_EFFECT_KIND,
                CHAT_BADGE_SLOT,
                CHAT_FLAG_SLOT,
                TITLE_EFFECT_KIND,
            ],
        )
        .await?;
        let mut active_username_effect: Option<ActiveUsernameEffect> = None;
        let mut active_badge_rental: Option<ActiveRental> = None;
        let mut active_flag_rental: Option<ActiveRental> = None;
        let mut active_title: Option<ActiveRental> = None;
        for row in rental_rows {
            match row.effect_kind.as_str() {
                USERNAME_EFFECT_KIND if active_username_effect.is_none() => {
                    active_username_effect =
                        UsernameEffect::from_payload(&row.payload).map(|effect| {
                            ActiveUsernameEffect {
                                effect,
                                ends_at: row.ends_at,
                            }
                        });
                }
                CHAT_BADGE_SLOT if active_badge_rental.is_none() => {
                    active_badge_rental = rental_from_effect_row(&row, |payload| {
                        BadgeRental::from_payload(payload).map(|rental| rental.emoji)
                    });
                }
                CHAT_FLAG_SLOT if active_flag_rental.is_none() => {
                    active_flag_rental = rental_from_effect_row(&row, |payload| {
                        BadgeRental::from_payload(payload).map(|rental| rental.emoji)
                    });
                }
                TITLE_EFFECT_KIND if active_title.is_none() => {
                    active_title = rental_from_effect_row(&row, title_from_payload);
                }
                _ => {}
            }
        }

        // What this user's chat label shows, from the one query that decides
        // it for everyone. Read here rather than derived from the catalog so
        // the buyer's own session never disagrees with what other people see.
        let (chat_label_badge, chat_label_flag) =
            match User::list_chat_author_metadata(&client, &[user_id])
                .await?
                .into_iter()
                .next()
            {
                Some(metadata) => (metadata.chat_badge, metadata.chat_flag),
                None => (None, None),
            };

        let active_bonsai_decay_protection =
            BonsaiDecayProtection::for_user(&client, user_id).await?;

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
                let is_rental = matches!(
                    item_kind.as_str(),
                    USERNAME_EFFECT_ITEM_KIND | BADGE_RENTAL_ITEM_KIND | TITLE_RENTAL_ITEM_KIND
                );
                let rental_duration_secs = is_rental.then(|| rental_duration_secs(&item));
                // A rental names the slot it fills in its payload, since it
                // never equips anything. Nothing else on sale fills one: the
                // Badges and Flags tabs are rentals top to bottom.
                let badge_slot = match item_kind.as_str() {
                    BADGE_RENTAL_ITEM_KIND => item
                        .payload
                        .get("slot")
                        .and_then(|value| value.as_str())
                        .filter(|slot| matches!(*slot, CHAT_BADGE_SLOT | CHAT_FLAG_SLOT))
                        .map(ToOwned::to_owned),
                    _ => None,
                };
                let custom_title =
                    item_kind == TITLE_RENTAL_ITEM_KIND && is_custom_title(&item.payload);
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
                    rental_duration_secs,
                    badge_slot,
                    custom_title,
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
            active_bonsai_decay_protection,
            active_badge_rental,
            active_flag_rental,
            active_title,
            chat_label_badge,
            chat_label_flag,
            custom_titles_available: self.custom_titles_enabled(),
        })
    }

    /// Whether a buyer-written title can be screened on this process. Read
    /// from local config, so it is the same answer on every replica running
    /// the same deployment.
    fn custom_titles_enabled(&self) -> bool {
        self.ai_service.as_ref().is_some_and(AiService::is_enabled)
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

/// One live effect row as the Shop shows it, when its payload still parses.
/// A row whose payload no longer renders is dropped rather than shown blank.
fn rental_from_effect_row(
    row: &ShopConsumableEffect,
    label: impl Fn(&serde_json::Value) -> Option<String>,
) -> Option<ActiveRental> {
    label(&row.payload).map(|label| ActiveRental {
        label,
        source_sku: row.source_sku.clone(),
        ends_at: row.ends_at,
    })
}

fn is_consumable_kind(item_kind: &str) -> bool {
    matches!(
        item_kind,
        CHAT_CONSUMABLE_ITEM_KIND | COMPANION_CONSUMABLE_ITEM_KIND | BONSAI_CONSUMABLE_ITEM_KIND
    )
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
