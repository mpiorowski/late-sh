use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::{
    chips::{ChipMove, UserChips},
    rental::{
        BADGE_RENTAL_ITEM_KIND, BadgeRental, CustomTitle, RENTAL_DAY_SECS, TITLE_EFFECT_KIND,
        TITLE_RENTAL_ITEM_KIND, is_custom_title, title_from_payload,
    },
    shop_consumable_effect::ShopConsumableEffect,
    username_effect::{USERNAME_EFFECT_KIND, UsernameEffect},
};

pub const PET_COMPANION_SKU: &str = "pet_companion";
pub const BONSAI_CONSUMABLE_ITEM_KIND: &str = "bonsai_consumable";
pub const BONSAI_DECAY_SHIELD_SKU: &str = "bonsai_decay_shield_two_weeks";
/// `shop_consumable_effects.effect_kind` for the user-scoped Bonsai Decay
/// Shield: while a live row of this kind covers a calendar day, that day
/// counts as cared-for against both bonsai decay clocks (classic dry-day
/// death, Dynamic vigor/water-stress decay), regardless of watering.
pub const BONSAI_DECAY_PROTECTION_KIND: &str = "bonsai_decay_protection";
/// Default protection window when an item payload omits `duration_secs`: 14
/// days.
pub const BONSAI_DECAY_PROTECTION_DURATION_SECS: i64 = 1_209_600;
pub const AQUARIUM_SKU: &str = "aquarium";
pub const AQUARIUM_FISH_ITEM_KIND: &str = "aquarium_fish";
/// The tank's plants (migration 183): bought like fish and capped apart
/// from them, and what a sprout roots as. They never starve and never
/// parent a fry; only the fish kind is in the care rolls.
pub const AQUARIUM_PLANT_ITEM_KIND: &str = "aquarium_plant";
/// The most fish a user can own, in the water or parked, and the most in
/// the water at once: one number, so the inventory never holds more than
/// a tank's worth. Buying, a hatch, and a rooting all stop at it.
pub const AQUARIUM_MAX_FISH: i32 = 20;
/// The plants' cap, the same shape as the fish's.
pub const AQUARIUM_MAX_PLANTS: i32 = 20;
/// The fish every tank comes with (migration 182): the fry, its own
/// catalog row so the shop shows the hatchling sprite that swims; listed,
/// never sold; `is_welcome_fish` reads the same fact off the payload.
pub const AQUARIUM_WELCOME_FISH_SKU: &str = "aquarium_fish_fry";
/// The Shop's row for the sprout on the tank floor (migration 182): listed
/// so the tank is tended in one place, never sold, never a purchase row;
/// its state lives on the care row.
pub const AQUARIUM_SPROUT_SKU: &str = "aquarium_sprout";

/// The two kinds of stock a tank holds, each with its own active cap:
/// what `adjust_aquarium_active_by_sku` and the growth paths count
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankStockKind {
    Fish,
    Plant,
}

impl TankStockKind {
    /// The kind an item kind names, `None` for anything that is not tank
    /// stock.
    pub fn of(item_kind: &str) -> Option<Self> {
        match item_kind {
            AQUARIUM_FISH_ITEM_KIND => Some(Self::Fish),
            AQUARIUM_PLANT_ITEM_KIND => Some(Self::Plant),
            _ => None,
        }
    }

    pub fn item_kind(self) -> &'static str {
        match self {
            Self::Fish => AQUARIUM_FISH_ITEM_KIND,
            Self::Plant => AQUARIUM_PLANT_ITEM_KIND,
        }
    }

    /// How many of this kind a user can own, and how many can be in the
    /// water at once: the same number.
    pub fn cap(self) -> i32 {
        match self {
            Self::Fish => AQUARIUM_MAX_FISH,
            Self::Plant => AQUARIUM_MAX_PLANTS,
        }
    }
}
pub const AQUARIUM_CONSUMABLE_ITEM_KIND: &str = "aquarium_consumable";
pub const AQUARIUM_SHIELD_SKU: &str = "aquarium_shield_two_weeks";
/// `shop_consumable_effects.effect_kind` for the user-scoped Aquarium
/// Shield: an auto feeder. A calendar day a row of this kind covers counts
/// as neither fed nor unfed for the tank's care clocks, so nothing starves
/// and the water stays clean while the owner is away.
pub const AQUARIUM_SHIELD_KIND: &str = "aquarium_shield";
/// Default shield window when an item payload omits `duration_secs`: 14 days.
pub const AQUARIUM_SHIELD_DURATION_SECS: i64 = 1_209_600;
pub const CHAT_CONSUMABLE_ITEM_KIND: &str = "chat_consumable";
pub const USERNAME_EFFECT_ITEM_KIND: &str = "username_effect";
pub const CHAT_BADGE_SLOT: &str = "chat_badge";
pub const CHAT_FLAG_SLOT: &str = "chat_flag";
pub const COMPANION_CONSUMABLE_ITEM_KIND: &str = "companion_consumable";
pub const ULTIMATE_SPELL_KIND: &str = "ultimate_spell";
pub const WONDERLAND_ULTIMATE_SKU: &str = "ultimate_wonderland";
pub const THEMATRIX_ULTIMATE_SKU: &str = "ultimate_thematrix";
pub const SHOP_USER_CHANGED_CHANNEL: &str = "shop_user_changed";
pub const SHOP_CATALOG_CHANGED_CHANNEL: &str = "shop_catalog_changed";

#[derive(Debug, Clone)]
pub struct MarketplaceItem {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub sku: String,
    pub item_kind: String,
    pub slot: Option<String>,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub payload: Value,
    pub active: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub sort_order: i32,
}

impl From<tokio_postgres::Row> for MarketplaceItem {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            sku: row.get("sku"),
            item_kind: row.get("item_kind"),
            slot: row.get("slot"),
            name: row.get("name"),
            description: row.get("description"),
            price_chips: row.get("price_chips"),
            payload: row.get("payload"),
            active: row.get("active"),
            starts_at: row.get("starts_at"),
            ends_at: row.get("ends_at"),
            sort_order: row.get("sort_order"),
        }
    }
}

impl MarketplaceItem {
    /// One purchasable item by SKU, under the same visibility rule as
    /// `list_visible`. `None` for an unknown, retired, or out-of-window SKU.
    pub async fn find_visible_by_sku(client: &Client, sku: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM marketplace_items
                 WHERE sku = $1
                   AND active = true
                   AND (starts_at IS NULL OR starts_at <= current_timestamp)
                   AND (ends_at IS NULL OR ends_at > current_timestamp)",
                &[&sku],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_visible(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM marketplace_items
                 WHERE active = true
                   AND (starts_at IS NULL OR starts_at <= current_timestamp)
                   AND (ends_at IS NULL OR ends_at > current_timestamp)
                 ORDER BY sort_order ASC, created ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}

#[derive(Debug, Clone)]
pub struct UserPurchase {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub quantity: i32,
    pub active_quantity: i32,
    pub remaining_uses: Option<i32>,
    pub equipped_slot: Option<String>,
    pub equipped_at: Option<DateTime<Utc>>,
    pub purchased_price_chips: i64,
}

impl From<tokio_postgres::Row> for UserPurchase {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            user_id: row.get("user_id"),
            item_id: row.get("item_id"),
            quantity: row.get("quantity"),
            active_quantity: row.get("active_quantity"),
            remaining_uses: row.get("remaining_uses"),
            equipped_slot: row.get("equipped_slot"),
            equipped_at: row.get("equipped_at"),
            purchased_price_chips: row.get("purchased_price_chips"),
        }
    }
}

impl UserPurchase {
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM user_purchases
                 WHERE user_id = $1
                 ORDER BY created DESC",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}

pub async fn listen_for_shop_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!(
            "LISTEN {SHOP_USER_CHANGED_CHANNEL};
             LISTEN {SHOP_CATALOG_CHANGED_CHANNEL};"
        ))
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseStatus {
    Purchased,
    QuantityAdded,
    AlreadyOwned,
    InsufficientFunds,
    RequiresAquarium,
    DailyLimitReached,
    /// A fish or plant the user already owns the cap of
    /// (`TankStockKind::cap`), in the water and parked together.
    OwnedCapReached,
}

#[derive(Debug, Clone)]
pub struct PurchaseResult {
    pub status: PurchaseStatus,
    pub item: MarketplaceItem,
    pub balance: i64,
    pub quantity: i32,
    pub active_quantity: i32,
}

#[derive(Debug, Clone)]
pub struct PurchaseWithEffectResult {
    pub purchase: Option<PurchaseResult>,
    pub refresh_all_active_users: bool,
    /// The user-scoped username-effect row activated by this purchase, when
    /// the bought item is a `username_effect`.
    pub username_effect: Option<ShopConsumableEffect>,
    /// The user-scoped Bonsai Decay Shield row activated (or extended) by
    /// this purchase, when the bought item is a `bonsai_consumable`.
    pub bonsai_decay_protection: Option<ShopConsumableEffect>,
    /// The user-scoped Aquarium Shield row activated (or extended) by this
    /// purchase, when the bought item is an `aquarium_consumable`.
    pub aquarium_shield: Option<ShopConsumableEffect>,
    /// The user-scoped chat badge or flag rental activated by this purchase,
    /// when the bought item is a `badge_rental`.
    pub badge_rental: Option<ShopConsumableEffect>,
    /// The user-scoped title rental activated by this purchase, when the
    /// bought item is a `title_rental`.
    pub title_rental: Option<ShopConsumableEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankActiveStatus {
    Changed,
    NotOwned,
    /// The sku is neither a fish nor a plant.
    NotTankStock,
    AtZero,
    AtOwnedQuantity,
    TankFull,
}

#[derive(Debug, Clone)]
pub struct TankActiveResult {
    pub status: TankActiveStatus,
    pub item: MarketplaceItem,
    pub quantity: i32,
    pub active_quantity: i32,
}

pub async fn purchase_durable_item_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
) -> Result<Option<PurchaseResult>> {
    Ok(
        purchase_item_by_sku_inner(client, user_id, sku, None, None, None)
            .await?
            .purchase,
    )
}

pub async fn purchase_item_by_sku_with_chat_effect(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    room_id: Option<Uuid>,
) -> Result<PurchaseWithEffectResult> {
    purchase_item_by_sku_inner(client, user_id, sku, Some(room_id), None, None).await
}

pub async fn purchase_item_by_sku_with_username_effect(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    effect: UsernameEffect,
) -> Result<PurchaseWithEffectResult> {
    purchase_item_by_sku_inner(client, user_id, sku, None, Some(effect), None).await
}

/// Rent a title the buyer wrote themselves. The text arrives as a
/// `CustomTitle`, so it has already passed every rule a rendered title must
/// obey; what this path still enforces is that the SKU actually sells a custom
/// title, and it does so inside the transaction, so a mismatch rolls the debit
/// back instead of charging for a title nothing would wear.
pub async fn purchase_item_by_sku_with_custom_title(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    title: CustomTitle,
) -> Result<PurchaseWithEffectResult> {
    purchase_item_by_sku_inner(client, user_id, sku, None, None, Some(title)).await
}

async fn purchase_item_by_sku_inner(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    chat_effect_room_id: Option<Option<Uuid>>,
    username_effect: Option<UsernameEffect>,
    custom_title: Option<CustomTitle>,
) -> Result<PurchaseWithEffectResult> {
    let tx = client.transaction().await?;

    let Some(item_row) = tx
        .query_opt(
            "SELECT *
             FROM marketplace_items
             WHERE sku = $1
               AND active = true
               AND (starts_at IS NULL OR starts_at <= current_timestamp)
               AND (ends_at IS NULL OR ends_at > current_timestamp)
             FOR UPDATE",
            &[&sku],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(PurchaseWithEffectResult {
            purchase: None,
            refresh_all_active_users: false,
            username_effect: None,
            bonsai_decay_protection: None,
            aquarium_shield: None,
            badge_rental: None,
            title_rental: None,
        });
    };
    let item = MarketplaceItem::from(item_row);
    if is_listed_only(&item.payload) {
        bail!("{} is shown in the shop but not for sale", item.sku);
    }
    let is_repeatable = is_repeatable_purchase_item(&item);
    let balance = lock_user_chips_in_tx(&tx, user_id).await?;

    let existing = tx
        .query_opt(
            "SELECT quantity, active_quantity
             FROM user_purchases
             WHERE user_id = $1 AND item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?;

    if let Some(kind) = TankStockKind::of(&item.item_kind) {
        let aquarium_owned = tx
            .query_opt(
                "SELECT 1
                 FROM user_purchases p
                 JOIN marketplace_items i ON i.id = p.item_id
                 WHERE p.user_id = $1 AND i.sku = $2",
                &[&user_id, &AQUARIUM_SKU],
            )
            .await?
            .is_some();
        if !aquarium_owned {
            tx.commit().await?;
            return Ok(PurchaseWithEffectResult {
                purchase: Some(PurchaseResult {
                    status: PurchaseStatus::RequiresAquarium,
                    item,
                    balance,
                    quantity: 0,
                    active_quantity: 0,
                }),
                refresh_all_active_users: false,
                username_effect: None,
                bonsai_decay_protection: None,
                aquarium_shield: None,
                badge_rental: None,
                title_rental: None,
            });
        }
        if aquarium_owned_quantity_in_tx(&tx, user_id, kind).await? >= kind.cap() {
            let (quantity, active_quantity) = match &existing {
                Some(row) => (
                    row.get::<_, i32>("quantity"),
                    row.get::<_, i32>("active_quantity"),
                ),
                None => (0, 0),
            };
            tx.commit().await?;
            return Ok(PurchaseWithEffectResult {
                purchase: Some(PurchaseResult {
                    status: PurchaseStatus::OwnedCapReached,
                    item,
                    balance,
                    quantity,
                    active_quantity,
                }),
                refresh_all_active_users: false,
                username_effect: None,
                bonsai_decay_protection: None,
                aquarium_shield: None,
                badge_rental: None,
                title_rental: None,
            });
        }
    }

    if let Some(existing) = existing {
        let quantity = existing.get::<_, i32>("quantity");
        let active_quantity = existing.get::<_, i32>("active_quantity");
        if !is_repeatable {
            tx.commit().await?;
            return Ok(PurchaseWithEffectResult {
                purchase: Some(PurchaseResult {
                    status: PurchaseStatus::AlreadyOwned,
                    item,
                    balance,
                    quantity,
                    active_quantity,
                }),
                refresh_all_active_users: false,
                username_effect: None,
                bonsai_decay_protection: None,
                aquarium_shield: None,
                badge_rental: None,
                title_rental: None,
            });
        }

        if has_reached_daily_purchase_limit(&tx, user_id, &item).await? {
            tx.commit().await?;
            return Ok(PurchaseWithEffectResult {
                purchase: Some(PurchaseResult {
                    status: PurchaseStatus::DailyLimitReached,
                    item,
                    balance,
                    quantity,
                    active_quantity,
                }),
                refresh_all_active_users: false,
                username_effect: None,
                bonsai_decay_protection: None,
                aquarium_shield: None,
                badge_rental: None,
                title_rental: None,
            });
        }

        if balance < item.price_chips {
            tx.commit().await?;
            return Ok(PurchaseWithEffectResult {
                purchase: Some(PurchaseResult {
                    status: PurchaseStatus::InsufficientFunds,
                    item,
                    balance,
                    quantity,
                    active_quantity,
                }),
                refresh_all_active_users: false,
                username_effect: None,
                bonsai_decay_protection: None,
                aquarium_shield: None,
                badge_rental: None,
                title_rental: None,
            });
        }

        let new_balance = match UserChips::apply(
            &tx,
            user_id,
            ChipMove::ShopPurchase,
            item.price_chips,
            &item.sku,
        )
        .await?
        {
            Some(chips) => chips.balance,
            None => anyhow::bail!("shop purchase debit failed despite locked balance"),
        };
        tx.execute(
            "UPDATE user_purchases
             SET quantity = quantity + 1,
                 purchased_price_chips = $3,
                 updated = current_timestamp
             WHERE user_id = $1 AND item_id = $2",
            &[&user_id, &item.id, &item.price_chips],
        )
        .await?;
        let refresh_all_active_users =
            activate_chat_consumable_in_tx(&tx, user_id, &item, chat_effect_room_id).await?;
        let activated_username_effect =
            activate_username_effect_in_tx(&tx, user_id, &item, username_effect).await?;
        let activated_bonsai_decay_protection =
            activate_bonsai_decay_protection_in_tx(&tx, user_id, &item).await?;
        let activated_aquarium_shield = activate_aquarium_shield_in_tx(&tx, user_id, &item).await?;
        let activated_badge_rental = activate_badge_rental_in_tx(&tx, user_id, &item).await?;
        let activated_title_rental =
            activate_title_rental_in_tx(&tx, user_id, &item, custom_title).await?;
        let payload = user_id.to_string();
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_USER_CHANGED_CHANNEL, &payload],
        )
        .await?;
        if refresh_all_active_users {
            tx.execute(
                "SELECT pg_notify($1, $2)",
                &[&SHOP_CATALOG_CHANGED_CHANNEL, &item.sku],
            )
            .await?;
        }
        tx.commit().await?;
        return Ok(PurchaseWithEffectResult {
            purchase: Some(PurchaseResult {
                status: PurchaseStatus::QuantityAdded,
                item,
                balance: new_balance,
                quantity: quantity + 1,
                active_quantity,
            }),
            refresh_all_active_users,
            username_effect: activated_username_effect,
            bonsai_decay_protection: activated_bonsai_decay_protection,
            aquarium_shield: activated_aquarium_shield,
            badge_rental: activated_badge_rental,
            title_rental: activated_title_rental,
        });
    }

    if has_reached_daily_purchase_limit(&tx, user_id, &item).await? {
        tx.commit().await?;
        return Ok(PurchaseWithEffectResult {
            purchase: Some(PurchaseResult {
                status: PurchaseStatus::DailyLimitReached,
                item,
                balance,
                quantity: 0,
                active_quantity: 0,
            }),
            refresh_all_active_users: false,
            username_effect: None,
            bonsai_decay_protection: None,
            aquarium_shield: None,
            badge_rental: None,
            title_rental: None,
        });
    }

    if balance < item.price_chips {
        tx.commit().await?;
        return Ok(PurchaseWithEffectResult {
            purchase: Some(PurchaseResult {
                status: PurchaseStatus::InsufficientFunds,
                item,
                balance,
                quantity: 0,
                active_quantity: 0,
            }),
            refresh_all_active_users: false,
            username_effect: None,
            bonsai_decay_protection: None,
            aquarium_shield: None,
            badge_rental: None,
            title_rental: None,
        });
    }

    let new_balance = match UserChips::apply(
        &tx,
        user_id,
        ChipMove::ShopPurchase,
        item.price_chips,
        &item.sku,
    )
    .await?
    {
        Some(chips) => chips.balance,
        None => anyhow::bail!("shop purchase debit failed despite locked balance"),
    };

    let active_quantity = 0;
    tx.execute(
        "INSERT INTO user_purchases
            (user_id, item_id, quantity, active_quantity, remaining_uses, equipped_slot, purchased_price_chips)
         VALUES ($1, $2, 1, $3, NULL, NULL, $4)",
        &[&user_id, &item.id, &active_quantity, &item.price_chips],
    )
    .await?;

    if let Some(slot) = &item.slot {
        equip_purchase_in_tx(&tx, user_id, item.id, slot).await?;
    }
    if item.sku == PET_COMPANION_SKU {
        tx.execute(
            "INSERT INTO pet_companions (user_id, adopted_at)
             VALUES ($1, current_timestamp)
             ON CONFLICT (user_id) DO UPDATE
             SET adopted_at = COALESCE(pet_companions.adopted_at, current_timestamp),
                 updated = current_timestamp",
            &[&user_id],
        )
        .await?;
    }
    // The tank comes with its first sprout on the floor and a fry in the
    // water, so it is never bought empty.
    if item.sku == AQUARIUM_SKU {
        super::aquarium_care::AquariumCare::welcome(&tx, user_id).await?;
        welcome_aquarium_fry_in_tx(&tx, user_id).await?;
    }

    let refresh_all_active_users =
        activate_chat_consumable_in_tx(&tx, user_id, &item, chat_effect_room_id).await?;
    let activated_username_effect =
        activate_username_effect_in_tx(&tx, user_id, &item, username_effect).await?;
    let activated_bonsai_decay_protection =
        activate_bonsai_decay_protection_in_tx(&tx, user_id, &item).await?;
    let activated_aquarium_shield = activate_aquarium_shield_in_tx(&tx, user_id, &item).await?;
    let activated_badge_rental = activate_badge_rental_in_tx(&tx, user_id, &item).await?;
    let activated_title_rental =
        activate_title_rental_in_tx(&tx, user_id, &item, custom_title).await?;
    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;
    if refresh_all_active_users {
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_CATALOG_CHANGED_CHANNEL, &item.sku],
        )
        .await?;
    }

    tx.commit().await?;
    Ok(PurchaseWithEffectResult {
        purchase: Some(PurchaseResult {
            status: PurchaseStatus::Purchased,
            item,
            balance: new_balance,
            quantity: 1,
            active_quantity,
        }),
        refresh_all_active_users,
        username_effect: activated_username_effect,
        bonsai_decay_protection: activated_bonsai_decay_protection,
        aquarium_shield: activated_aquarium_shield,
        badge_rental: activated_badge_rental,
        title_rental: activated_title_rental,
    })
}

/// Move one of the user's fish or plants between the water and the
/// inventory: `delta` is +1 or -1 on the sku's active count, bounded by
/// the owned count and the kind's cap (`TankStockKind::cap`).
pub async fn adjust_aquarium_active_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    delta: i32,
) -> Result<Option<TankActiveResult>> {
    let tx = client.transaction().await?;
    let Some(item_row) = tx
        .query_opt(
            "SELECT *
             FROM marketplace_items
             WHERE sku = $1",
            &[&sku],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };
    let item = MarketplaceItem::from(item_row);
    let Some(kind) = TankStockKind::of(&item.item_kind) else {
        tx.commit().await?;
        return Ok(Some(TankActiveResult {
            status: TankActiveStatus::NotTankStock,
            item,
            quantity: 0,
            active_quantity: 0,
        }));
    };
    let _balance = lock_user_chips_in_tx(&tx, user_id).await?;

    let Some(purchase_row) = tx
        .query_opt(
            "SELECT quantity, active_quantity
             FROM user_purchases
             WHERE user_id = $1 AND item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(Some(TankActiveResult {
            status: TankActiveStatus::NotOwned,
            item,
            quantity: 0,
            active_quantity: 0,
        }));
    };

    let quantity = purchase_row.get::<_, i32>("quantity");
    let active_quantity = purchase_row.get::<_, i32>("active_quantity");
    if delta < 0 && active_quantity == 0 {
        tx.commit().await?;
        return Ok(Some(TankActiveResult {
            status: TankActiveStatus::AtZero,
            item,
            quantity,
            active_quantity,
        }));
    }
    if delta > 0 && active_quantity >= quantity {
        tx.commit().await?;
        return Ok(Some(TankActiveResult {
            status: TankActiveStatus::AtOwnedQuantity,
            item,
            quantity,
            active_quantity,
        }));
    }
    let next_active = active_quantity.saturating_add(delta).clamp(0, quantity);
    if delta > 0 {
        let current_total = aquarium_active_quantity_in_tx(&tx, user_id, kind).await?;
        let projected_total = current_total
            .saturating_sub(active_quantity)
            .saturating_add(next_active);
        if projected_total > kind.cap() {
            tx.commit().await?;
            return Ok(Some(TankActiveResult {
                status: TankActiveStatus::TankFull,
                item,
                quantity,
                active_quantity,
            }));
        }
    }

    tx.execute(
        "UPDATE user_purchases
         SET active_quantity = $3, updated = current_timestamp
         WHERE user_id = $1 AND item_id = $2",
        &[&user_id, &item.id, &next_active],
    )
    .await?;
    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;
    tx.commit().await?;
    Ok(Some(TankActiveResult {
        status: TankActiveStatus::Changed,
        item,
        quantity,
        active_quantity: next_active,
    }))
}

/// Active aquarium creatures `(creature_name, count)` a user is currently
/// displaying, fish and plants alike. Mirrors
/// `ShopState::active_aquarium_creatures` but reads from the database for
/// an arbitrary user, so profile views can render someone else's tank.
pub async fn active_aquarium_creatures_for_user(
    client: &Client,
    user_id: Uuid,
) -> Result<Vec<(String, usize)>> {
    let rows = client
        .query(
            "SELECT i.payload->>'creature' AS creature,
                    p.active_quantity AS count
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1
               AND i.item_kind IN ($2, $3)
               AND p.active_quantity > 0
               AND i.payload->>'creature' IS NOT NULL
             ORDER BY creature",
            &[
                &user_id,
                &AQUARIUM_FISH_ITEM_KIND,
                &AQUARIUM_PLANT_ITEM_KIND,
            ],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let creature: Option<String> = row.get("creature");
            let count: i32 = row.get("count");
            creature
                .filter(|creature| !creature.is_empty())
                .map(|creature| (creature, count.max(0) as usize))
        })
        .collect())
}

/// How many of one kind the user owns, in the water or parked, for the
/// kind's cap.
async fn aquarium_owned_quantity_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    kind: TankStockKind,
) -> Result<i32> {
    let row = tx
        .query_one(
            "SELECT COALESCE(SUM(p.quantity), 0)::INT AS total
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.item_kind = $2",
            &[&user_id, &kind.item_kind()],
        )
        .await?;
    Ok(row.get("total"))
}

/// How many of one kind are in the user's water, for the kind's cap.
async fn aquarium_active_quantity_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    kind: TankStockKind,
) -> Result<i32> {
    let row = tx
        .query_one(
            "SELECT COALESCE(SUM(p.active_quantity), 0)::INT AS total
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.item_kind = $2",
            &[&user_id, &kind.item_kind()],
        )
        .await?;
    Ok(row.get("total"))
}

/// A catalog fish the shop shows but never sells: the one the tank comes
/// with (`payload.welcome`, `AQUARIUM_WELCOME_FISH_SKU`).
pub fn is_welcome_fish(payload: &Value) -> bool {
    payload.get("welcome").and_then(Value::as_bool) == Some(true)
}

/// The Shop's sprout row (`payload.sprout`, `AQUARIUM_SPROUT_SKU`).
pub fn is_sprout_row(payload: &Value) -> bool {
    payload.get("sprout").and_then(Value::as_bool) == Some(true)
}

/// A catalog item the Shop lists but the purchase path refuses: the
/// welcome fry and the sprout row.
pub fn is_listed_only(payload: &Value) -> bool {
    is_welcome_fish(payload) || is_sprout_row(payload)
}

fn is_repeatable_purchase_item(item: &MarketplaceItem) -> bool {
    matches!(
        item.item_kind.as_str(),
        AQUARIUM_FISH_ITEM_KIND
            | AQUARIUM_PLANT_ITEM_KIND
            | CHAT_CONSUMABLE_ITEM_KIND
            | COMPANION_CONSUMABLE_ITEM_KIND
            | USERNAME_EFFECT_ITEM_KIND
            | BONSAI_CONSUMABLE_ITEM_KIND
            | AQUARIUM_CONSUMABLE_ITEM_KIND
            | BADGE_RENTAL_ITEM_KIND
            | TITLE_RENTAL_ITEM_KIND
    )
}

/// Activates the chat consumable bought in this transaction. Returns whether
/// every active user's snapshot must be reloaded, which a room effect always
/// needs: it is projected into every viewer's snapshot, not only the buyer's.
///
/// Every chat consumable has to be room-targeted. `shop_consumable_effects` can
/// physically hold user-scoped rows, but nothing projects them into a snapshot,
/// so a user-scoped item would take the chips and do nothing anyone could see.
/// Fail the purchase transaction rather than charge for a no-op.
async fn activate_chat_consumable_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
    chat_effect_room_id: Option<Option<Uuid>>,
) -> Result<bool> {
    let Some(room_id) = chat_effect_room_id else {
        return Ok(false);
    };
    if item.item_kind != CHAT_CONSUMABLE_ITEM_KIND {
        return Ok(false);
    }

    let Some(effect_kind) = item
        .payload
        .get("effect_kind")
        .and_then(|value| value.as_str())
        .filter(|effect_kind| !effect_kind.trim().is_empty())
    else {
        bail!("chat consumable {} is missing effect_kind", item.sku);
    };
    let duration_secs = item
        .payload
        .get("duration_secs")
        .and_then(|value| value.as_i64())
        .unwrap_or(1);
    if item.payload.get("target").and_then(|value| value.as_str()) != Some("room") {
        bail!("chat consumable {} must target a room", item.sku);
    }
    let Some(room_id) = room_id else {
        bail!("room-targeted consumable {} requires a room", item.sku);
    };

    ShopConsumableEffect::activate_room_effect_in_tx(
        tx,
        user_id,
        room_id,
        effect_kind,
        &item.sku,
        duration_secs,
        item.payload.clone(),
    )
    .await?;
    Ok(true)
}

/// Activates the username effect bought in this transaction. A username
/// effect purchase must carry the buyer's style choice, and the choice must
/// belong to the bought item's variant; failing the transaction means the
/// buyer is never charged for an effect that could not activate. Any prior
/// live username effect is replaced (one active effect per user).
async fn activate_username_effect_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
    choice: Option<UsernameEffect>,
) -> Result<Option<ShopConsumableEffect>> {
    if item.item_kind != USERNAME_EFFECT_ITEM_KIND {
        return Ok(None);
    }
    let Some(choice) = choice else {
        bail!("username effect {} requires a style choice", item.sku);
    };
    let variant = item.payload.get("variant").and_then(|value| value.as_str());
    if variant != Some(choice.variant_key()) {
        bail!(
            "username effect {} does not accept style {}",
            item.sku,
            choice.slug()
        );
    }
    let effect = ShopConsumableEffect::activate_user_effect_in_tx(
        tx,
        user_id,
        USERNAME_EFFECT_KIND,
        &item.sku,
        rental_duration_secs(item),
        choice.to_payload(),
    )
    .await?;
    Ok(Some(effect))
}

/// How long the rental this item sells runs. The day tier carries 86400 and
/// the month tier 2592000 in its payload; an item that carries neither falls
/// back to the day window rather than activating forever. Shared by every
/// rental kind (username effects, badge and flag rentals, titles) so the shop
/// never quotes a window the activation would not honour.
pub fn rental_duration_secs(item: &MarketplaceItem) -> i64 {
    super::rental::duration_secs(&item.payload, RENTAL_DAY_SECS)
}

/// Activates the chat badge or flag rental bought in this transaction. The
/// rental fills its slot through a user-scoped effect row rather than the
/// legacy `equipped_slot` path, so it expires on its own and a rebuy of any
/// badge for the same slot replaces the live row and resets the clock. A
/// payload nothing could render fails the transaction: never charge for a
/// badge that would not appear.
async fn activate_badge_rental_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
) -> Result<Option<ShopConsumableEffect>> {
    if item.item_kind != BADGE_RENTAL_ITEM_KIND {
        return Ok(None);
    }
    let Some(rental) = BadgeRental::from_payload(&item.payload) else {
        bail!("badge rental {} has no renderable emoji and slot", item.sku);
    };
    let effect = ShopConsumableEffect::activate_user_effect_in_tx(
        tx,
        user_id,
        rental.slot.effect_kind(),
        &item.sku,
        rental_duration_secs(item),
        item.payload.clone(),
    )
    .await?;
    Ok(Some(effect))
}

/// Activates the title rental bought in this transaction. One active title per
/// user, rebuy replaces, exactly the username-effect path; a title and a
/// username color are separate effect kinds, so buying one never clears the
/// other.
///
/// Where the text comes from is the SKU's call, not the caller's: a curated
/// SKU wears what its payload carries and refuses buyer text, a custom SKU
/// wears the buyer's text and refuses to activate without it. Either mismatch
/// fails the transaction, so nobody is charged for a title that could not be
/// the one they asked for.
async fn activate_title_rental_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
    custom_title: Option<CustomTitle>,
) -> Result<Option<ShopConsumableEffect>> {
    if item.item_kind != TITLE_RENTAL_ITEM_KIND {
        if custom_title.is_some() {
            bail!("{} is not a title rental and takes no title text", item.sku);
        }
        return Ok(None);
    }
    let text = match (is_custom_title(&item.payload), custom_title) {
        (true, Some(title)) => title.into_string(),
        (true, None) => bail!("custom title {} requires the buyer's text", item.sku),
        (false, Some(_)) => bail!("curated title {} takes no custom text", item.sku),
        (false, None) => match title_from_payload(&item.payload) {
            Some(text) => text,
            None => bail!("title rental {} carries no title text", item.sku),
        },
    };
    let effect = ShopConsumableEffect::activate_user_effect_in_tx(
        tx,
        user_id,
        TITLE_EFFECT_KIND,
        &item.sku,
        rental_duration_secs(item),
        serde_json::json!({"text": text}),
    )
    .await?;
    Ok(Some(effect))
}

/// Activates the Bonsai Decay Shield bought in this transaction. Unlike the
/// username effect above, a live protection window is extended by the
/// item's duration rather than reset (`extend_user_effect_in_tx`), so
/// stacking never discards time the player already paid for.
async fn activate_bonsai_decay_protection_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
) -> Result<Option<ShopConsumableEffect>> {
    if item.item_kind != BONSAI_CONSUMABLE_ITEM_KIND {
        return Ok(None);
    }
    let duration_secs = item
        .payload
        .get("duration_secs")
        .and_then(|value| value.as_i64())
        .unwrap_or(BONSAI_DECAY_PROTECTION_DURATION_SECS);

    let effect = ShopConsumableEffect::extend_user_effect_in_tx(
        tx,
        user_id,
        BONSAI_DECAY_PROTECTION_KIND,
        &item.sku,
        duration_secs,
        item.payload.clone(),
    )
    .await?;
    Ok(Some(effect))
}

/// Activates the Aquarium Shield bought in this transaction: the Bonsai
/// Decay Shield's shape, a live window is extended rather than reset.
async fn activate_aquarium_shield_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
) -> Result<Option<ShopConsumableEffect>> {
    if item.item_kind != AQUARIUM_CONSUMABLE_ITEM_KIND {
        return Ok(None);
    }
    let duration_secs = item
        .payload
        .get("duration_secs")
        .and_then(|value| value.as_i64())
        .unwrap_or(AQUARIUM_SHIELD_DURATION_SECS);

    let effect = ShopConsumableEffect::extend_user_effect_in_tx(
        tx,
        user_id,
        AQUARIUM_SHIELD_KIND,
        &item.sku,
        duration_secs,
        item.payload.clone(),
    )
    .await?;
    Ok(Some(effect))
}

/// Whether the user owns the Pet Companion: what puts a pet on a profile.
pub async fn user_owns_pet_companion(client: &impl GenericClient, user_id: Uuid) -> Result<bool> {
    let row = client
        .query_opt(
            "SELECT 1
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.sku = $2",
            &[&user_id, &PET_COMPANION_SKU],
        )
        .await?;
    Ok(row.is_some())
}

/// Whether the user owns the Aquarium feature.
pub async fn user_owns_aquarium(client: &impl GenericClient, user_id: Uuid) -> Result<bool> {
    let row = client
        .query_opt(
            "SELECT 1
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.sku = $2",
            &[&user_id, &AQUARIUM_SKU],
        )
        .await?;
    Ok(row.is_some())
}

/// One species in a user's tank: what the care rolls (fry, starvation)
/// weigh and act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FishStock {
    pub item_id: Uuid,
    pub creature: String,
    pub name: String,
    pub price_chips: i64,
    pub quantity: i32,
    pub active_quantity: i32,
}

/// Every species with at least one fish swimming in the user's tank, locked
/// for the transaction. Fish kept in inventory (owned, not active) are not
/// in the water, so they neither breed nor starve. Plants are another item
/// kind and never in this list: nothing roots from a fish or starves a
/// plant.
pub async fn swimming_fish_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
) -> Result<Vec<FishStock>> {
    let rows = tx
        .query(
            "SELECT p.item_id, i.payload->>'creature' AS creature, i.name, i.price_chips,
                    p.quantity, p.active_quantity
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1
               AND i.item_kind = $2
               AND p.active_quantity > 0
               AND i.payload->>'creature' IS NOT NULL
             ORDER BY i.sort_order, i.sku
             FOR UPDATE OF p",
            &[&user_id, &AQUARIUM_FISH_ITEM_KIND],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| FishStock {
            item_id: row.get("item_id"),
            creature: row.get("creature"),
            name: row.get("name"),
            price_chips: row.get("price_chips"),
            quantity: row.get("quantity"),
            active_quantity: row.get("active_quantity"),
        })
        .collect())
}

/// Where something the tank grew on its own (a fry, a rooting sprout)
/// ended up. The owned cap and the water's cap are one number
/// (`TankStockKind::cap`), so whatever is born has room in the water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankSpawn {
    /// Owned and in the water.
    Swimming,
    /// Not born at all: the user already owns the cap of its kind, in the
    /// water and parked together. Nothing was written.
    NoRoom,
}

/// A fry of `item_id` hatched: one more owned and swimming; nothing at all
/// when the owned fish are at `AQUARIUM_MAX_FISH` already.
pub async fn hatch_aquarium_fry_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<TankSpawn> {
    if aquarium_owned_quantity_in_tx(tx, user_id, TankStockKind::Fish).await? >= AQUARIUM_MAX_FISH {
        return Ok(TankSpawn::NoRoom);
    }
    let updated = tx
        .execute(
            "UPDATE user_purchases
             SET quantity = quantity + 1,
                 active_quantity = active_quantity + 1,
                 updated = current_timestamp
             WHERE user_id = $1 AND item_id = $2",
            &[&user_id, &item_id],
        )
        .await?;
    if updated != 1 {
        bail!("fry hatched for a species the user does not own");
    }
    Ok(TankSpawn::Swimming)
}

/// One plant the catalog sells: what a rooting sprout can become.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlantSpecies {
    pub item_id: Uuid,
    pub creature: String,
    pub name: String,
}

/// Every plant on sale, in catalog order: the sprout row is not one (it
/// has no creature of its own to grow), and neither is a retired plant.
pub async fn catalog_plants_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
) -> Result<Vec<PlantSpecies>> {
    let rows = tx
        .query(
            "SELECT id, payload->>'creature' AS creature, name
             FROM marketplace_items
             WHERE item_kind = $1
               AND active
               AND payload->>'creature' IS NOT NULL
               AND COALESCE((payload->>'sprout')::bool, false) = false
             ORDER BY sort_order, sku",
            &[&AQUARIUM_PLANT_ITEM_KIND],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| PlantSpecies {
            item_id: row.get("id"),
            creature: row.get("creature"),
            name: row.get("name"),
        })
        .collect())
}

/// The tank's welcome fish: one fry (`AQUARIUM_WELCOME_FISH_SKU`, the
/// hatchling row the shop never sells), owned and swimming from the
/// purchase at no price, and stamped on the care row as today's fry.
/// Returns the creature. Called inside the purchase transaction of a tank
/// nobody owned, so the buyer has no fish rows yet.
pub async fn welcome_aquarium_fry_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
) -> Result<String> {
    let row = tx
        .query_one(
            "SELECT id, payload->>'creature' AS creature
             FROM marketplace_items
             WHERE sku = $1 AND active",
            &[&AQUARIUM_WELCOME_FISH_SKU],
        )
        .await?;
    let item_id: Uuid = row.get("id");
    let creature: String = row.get("creature");
    tx.execute(
        "INSERT INTO user_purchases
            (user_id, item_id, quantity, active_quantity, remaining_uses, equipped_slot, purchased_price_chips)
         VALUES ($1, $2, 1, 1, NULL, NULL, 0)",
        &[&user_id, &item_id],
    )
    .await?;
    super::aquarium_care::AquariumCare::set_fry(tx, user_id, &creature, Utc::now().date_naive())
        .await?;
    Ok(creature)
}

/// A sprout the owner left alone rooted as the plant `item_id` (one of
/// `catalog_plants_in_tx`): one more owned and in the water; nothing at all
/// when the owned plants are at `AQUARIUM_MAX_PLANTS` already (the sprout
/// withers). A free plant, so the row's purchase price is zero when this
/// is the first of its kind.
pub async fn root_aquarium_sprout_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<TankSpawn> {
    if aquarium_owned_quantity_in_tx(tx, user_id, TankStockKind::Plant).await?
        >= AQUARIUM_MAX_PLANTS
    {
        return Ok(TankSpawn::NoRoom);
    }
    let updated = tx
        .execute(
            "UPDATE user_purchases
             SET quantity = quantity + 1,
                 active_quantity = active_quantity + 1,
                 updated = current_timestamp
             WHERE user_id = $1 AND item_id = $2",
            &[&user_id, &item_id],
        )
        .await?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO user_purchases
                (user_id, item_id, quantity, active_quantity, remaining_uses, equipped_slot, purchased_price_chips)
             VALUES ($1, $2, 1, 1, NULL, NULL, 0)",
            &[&user_id, &item_id],
        )
        .await?;
    }
    Ok(TankSpawn::Swimming)
}

/// One swimming fish of `item_id` starved: gone from the water and from the
/// owned count. A no-op when none of that species is swimming.
pub async fn starve_aquarium_fish_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<()> {
    tx.execute(
        "UPDATE user_purchases
         SET quantity = quantity - 1,
             active_quantity = active_quantity - 1,
             updated = current_timestamp
         WHERE user_id = $1 AND item_id = $2 AND active_quantity > 0",
        &[&user_id, &item_id],
    )
    .await?;
    Ok(())
}

/// Tell every replica this user's purchases changed, so their shop snapshot
/// (and with it the tank's population) reloads.
pub async fn notify_user_shop_changed(client: &impl GenericClient, user_id: Uuid) -> Result<()> {
    let payload = user_id.to_string();
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_USER_CHANGED_CHANNEL, &payload],
        )
        .await?;
    Ok(())
}

async fn has_reached_daily_purchase_limit(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item: &MarketplaceItem,
) -> Result<bool> {
    let daily_limit = item
        .payload
        .get("daily_limit")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !daily_limit {
        return Ok(false);
    }

    let row = tx
        .query_one(
            "SELECT EXISTS (
                 SELECT 1
                 FROM chip_ledger
                 WHERE user_id = $1
                   AND reason = $2
                   AND source_kind = $3
                   AND source_ref = $4
                   AND created_at >= (date_trunc('day', current_timestamp AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')
             ) AS purchased_today",
            &[
                &user_id,
                &ChipMove::ShopPurchase.reason(),
                &ChipMove::ShopPurchase.source_kind(),
                &item.sku,
            ],
        )
        .await?;
    Ok(row.get("purchased_today"))
}

async fn lock_user_chips_in_tx(tx: &tokio_postgres::Transaction<'_>, user_id: Uuid) -> Result<i64> {
    UserChips::ensure_in(tx, user_id).await?;
    let row = tx
        .query_one(
            "SELECT balance
             FROM user_chips
             WHERE user_id = $1
             FOR UPDATE",
            &[&user_id],
        )
        .await?;
    Ok(row.get("balance"))
}

async fn equip_purchase_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    item_id: Uuid,
    slot: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE user_purchases p
         SET equipped_slot = NULL, updated = current_timestamp
         FROM marketplace_items i
         WHERE p.item_id = i.id
           AND p.user_id = $1
           AND i.slot = $2",
        &[&user_id, &slot],
    )
    .await?;
    tx.execute(
        "UPDATE user_purchases
         SET equipped_slot = $3, equipped_at = current_timestamp, updated = current_timestamp
         WHERE user_id = $1 AND item_id = $2",
        &[&user_id, &item_id, &slot],
    )
    .await?;
    Ok(())
}
