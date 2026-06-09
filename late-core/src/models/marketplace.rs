use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

use super::{chips::INITIAL_CHIP_BALANCE, shop_consumable_effect::ShopConsumableEffect};

pub const PET_COMPANION_SKU: &str = "pet_companion";
pub const DYNAMIC_BONSAI_SKU: &str = "dynamic_bonsai";
pub const BONSAI_VARIANT_SLOT: &str = "bonsai_variant";
pub const AQUARIUM_SKU: &str = "aquarium";
pub const AQUARIUM_FISH_ITEM_KIND: &str = "aquarium_fish";
pub const AQUARIUM_MAX_FISH: i32 = 20;
pub const AQUARIUM_FOOD_SKU: &str = "aquarium_food";
pub const AQUARIUM_HUNGER_AFTER_HOURS: i64 = 24;
pub const CHAT_CONSUMABLE_ITEM_KIND: &str = "chat_consumable";
pub const CHAT_BADGE_SLOT: &str = "chat_badge";
pub const CHAT_FLAG_SLOT: &str = "chat_flag";
pub const COMPANION_CONSUMABLE_ITEM_KIND: &str = "companion_consumable";
pub const PET_FOOD_SKU: &str = "pet_food";
pub const ULTIMATE_SPELL_KIND: &str = "ultimate_spell";
pub const WONDERLAND_ULTIMATE_SKU: &str = "ultimate_wonderland";
pub const THEMATRIX_ULTIMATE_SKU: &str = "ultimate_thematrix";
pub const SHOP_PURCHASE_REASON: &str = "shop_purchase";
pub const MARKETPLACE_SOURCE_KIND: &str = "marketplace_item";
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

#[derive(Debug, Clone)]
pub struct MarketplaceAdminRow {
    pub id: Uuid,
    pub sku: String,
    pub item_kind: String,
    pub slot: Option<String>,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub payload: Value,
    pub active: bool,
    pub sort_order: i32,
}

impl From<tokio_postgres::Row> for MarketplaceAdminRow {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            sku: row.get("sku"),
            item_kind: row.get("item_kind"),
            slot: row.get("slot"),
            name: row.get("name"),
            description: row.get("description"),
            price_chips: row.get("price_chips"),
            payload: row.get("payload"),
            active: row.get("active"),
            sort_order: row.get("sort_order"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarketplaceAdminUpdate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub active: bool,
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

pub async fn list_marketplace_items_for_admin(
    client: &impl deadpool_postgres::GenericClient,
) -> Result<Vec<MarketplaceAdminRow>> {
    let rows = client
        .query(
            "SELECT id, sku, item_kind, slot, name, description, price_chips,
                    payload, active, sort_order
             FROM marketplace_items
             ORDER BY item_kind ASC, sort_order ASC, sku ASC",
            &[],
        )
        .await?;
    Ok(rows.into_iter().map(MarketplaceAdminRow::from).collect())
}

pub async fn update_marketplace_item_for_admin(
    client: &impl deadpool_postgres::GenericClient,
    update: MarketplaceAdminUpdate,
) -> Result<MarketplaceAdminRow> {
    ensure!(!update.name.trim().is_empty(), "name cannot be empty");
    ensure!(
        !update.description.trim().is_empty(),
        "description cannot be empty"
    );
    ensure!(update.price_chips >= 0, "price must be 0 or greater");

    let row = client
        .query_opt(
            "UPDATE marketplace_items
             SET
                 name = $2,
                 description = $3,
                 price_chips = $4,
                 active = $5,
                 sort_order = $6,
                 updated = current_timestamp
             WHERE id = $1
             RETURNING id, sku, item_kind, slot, name, description, price_chips,
                       payload, active, sort_order",
            &[
                &update.id,
                &update.name.trim(),
                &update.description.trim(),
                &update.price_chips,
                &update.active,
                &update.sort_order,
            ],
        )
        .await?;
    let row = row
        .map(MarketplaceAdminRow::from)
        .with_context(|| format!("marketplace item {} not found", update.id))?;
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_CATALOG_CHANGED_CHANNEL, &row.sku],
        )
        .await?;
    Ok(row)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipStatus {
    Equipped,
    AlreadyEquipped,
    NotOwned,
    NotEquippable,
}

#[derive(Debug, Clone)]
pub struct EquipResult {
    pub status: EquipStatus,
    pub item: MarketplaceItem,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FishActiveStatus {
    Changed,
    NotOwned,
    NotFish,
    AtZero,
    AtOwnedQuantity,
    TankFull,
}

#[derive(Debug, Clone)]
pub struct FishActiveResult {
    pub status: FishActiveStatus,
    pub item: MarketplaceItem,
    pub quantity: i32,
    pub active_quantity: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumableUseStatus {
    Used,
    NotAvailable,
    NotConsumable,
    OutOfStock,
    DailyLimitReached,
}

#[derive(Debug, Clone)]
pub struct ConsumableUseResult {
    pub status: ConsumableUseStatus,
    pub quantity_remaining: i32,
}

pub async fn purchase_durable_item_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
) -> Result<Option<PurchaseResult>> {
    Ok(purchase_item_by_sku_inner(client, user_id, sku, None)
        .await?
        .purchase)
}

pub async fn purchase_item_by_sku_with_chat_effect(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    room_id: Option<Uuid>,
) -> Result<PurchaseWithEffectResult> {
    purchase_item_by_sku_inner(client, user_id, sku, Some(room_id)).await
}

async fn purchase_item_by_sku_inner(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    chat_effect_room_id: Option<Option<Uuid>>,
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
        });
    };
    let item = MarketplaceItem::from(item_row);
    let is_aquarium_fish = item.item_kind == AQUARIUM_FISH_ITEM_KIND;
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

    if is_aquarium_fish {
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
            });
        }

        let new_balance = balance - item.price_chips;
        tx.execute(
            "UPDATE user_chips
             SET balance = $2, updated = current_timestamp
             WHERE user_id = $1",
            &[&user_id, &new_balance],
        )
        .await?;
        tx.execute(
            "INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &user_id,
                &(-item.price_chips),
                &SHOP_PURCHASE_REASON,
                &MARKETPLACE_SOURCE_KIND,
                &item.sku,
            ],
        )
        .await?;
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
        });
    }

    let new_balance = balance - item.price_chips;
    tx.execute(
        "UPDATE user_chips
         SET balance = $2, updated = current_timestamp
         WHERE user_id = $1",
        &[&user_id, &new_balance],
    )
    .await?;

    tx.execute(
        "INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
         VALUES ($1, $2, $3, $4, $5)",
        &[
            &user_id,
            &(-item.price_chips),
            &SHOP_PURCHASE_REASON,
            &MARKETPLACE_SOURCE_KIND,
            &item.sku,
        ],
    )
    .await?;

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

    let refresh_all_active_users =
        activate_chat_consumable_in_tx(&tx, user_id, &item, chat_effect_room_id).await?;
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
    })
}

pub async fn adjust_aquarium_fish_active_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
    delta: i32,
) -> Result<Option<FishActiveResult>> {
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
    if item.item_kind != AQUARIUM_FISH_ITEM_KIND {
        tx.commit().await?;
        return Ok(Some(FishActiveResult {
            status: FishActiveStatus::NotFish,
            item,
            quantity: 0,
            active_quantity: 0,
        }));
    }
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
        return Ok(Some(FishActiveResult {
            status: FishActiveStatus::NotOwned,
            item,
            quantity: 0,
            active_quantity: 0,
        }));
    };

    let quantity = purchase_row.get::<_, i32>("quantity");
    let active_quantity = purchase_row.get::<_, i32>("active_quantity");
    if delta < 0 && active_quantity == 0 {
        tx.commit().await?;
        return Ok(Some(FishActiveResult {
            status: FishActiveStatus::AtZero,
            item,
            quantity,
            active_quantity,
        }));
    }
    if delta > 0 && active_quantity >= quantity {
        tx.commit().await?;
        return Ok(Some(FishActiveResult {
            status: FishActiveStatus::AtOwnedQuantity,
            item,
            quantity,
            active_quantity,
        }));
    }
    let next_active = active_quantity.saturating_add(delta).clamp(0, quantity);
    if delta > 0 {
        let current_total = aquarium_fish_active_quantity_in_tx(&tx, user_id).await?;
        let projected_total = current_total
            .saturating_sub(active_quantity)
            .saturating_add(next_active);
        if projected_total > AQUARIUM_MAX_FISH {
            tx.commit().await?;
            return Ok(Some(FishActiveResult {
                status: FishActiveStatus::TankFull,
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
    Ok(Some(FishActiveResult {
        status: FishActiveStatus::Changed,
        item,
        quantity,
        active_quantity: next_active,
    }))
}

pub async fn consume_pet_food_treat(
    client: &mut Client,
    user_id: Uuid,
) -> Result<ConsumableUseResult> {
    let tx = client.transaction().await?;
    let Some(item_row) = tx
        .query_opt(
            "SELECT *
             FROM marketplace_items
             WHERE sku = $1
             FOR UPDATE",
            &[&PET_FOOD_SKU],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::NotAvailable,
            quantity_remaining: 0,
        });
    };
    let item = MarketplaceItem::from(item_row);
    if item.item_kind != COMPANION_CONSUMABLE_ITEM_KIND
        || item
            .payload
            .get("effect_kind")
            .and_then(|value| value.as_str())
            != Some("pet_food")
    {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::NotConsumable,
            quantity_remaining: 0,
        });
    }

    let Some(purchase_row) = tx
        .query_opt(
            "SELECT p.quantity
             FROM user_purchases p
             WHERE p.user_id = $1 AND p.item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::OutOfStock,
            quantity_remaining: 0,
        });
    };
    let quantity = purchase_row.get::<_, i32>("quantity");
    if quantity <= 0 {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::OutOfStock,
            quantity_remaining: 0,
        });
    }

    tx.execute(
        "INSERT INTO pet_companions (user_id)
         VALUES ($1)
         ON CONFLICT (user_id) DO NOTHING",
        &[&user_id],
    )
    .await?;
    let companion_row = tx
        .query_one(
            "SELECT COALESCE(
                    last_treated >= (date_trunc('day', current_timestamp AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'),
                    false
                ) AS treated_today
             FROM pet_companions
             WHERE user_id = $1
             FOR UPDATE",
            &[&user_id],
        )
        .await?;
    let treated_today = companion_row.get::<_, bool>("treated_today");
    if treated_today {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::DailyLimitReached,
            quantity_remaining: quantity,
        });
    }

    let quantity_remaining = quantity - 1;
    tx.execute(
        "UPDATE user_purchases
         SET quantity = $3,
             active_quantity = LEAST(active_quantity, $3),
             updated = current_timestamp
         WHERE user_id = $1 AND item_id = $2",
        &[&user_id, &item.id, &quantity_remaining],
    )
    .await?;
    tx.execute(
        "UPDATE pet_companions
         SET last_treated = current_timestamp, updated = current_timestamp
         WHERE user_id = $1",
        &[&user_id],
    )
    .await?;
    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;
    tx.commit().await?;
    Ok(ConsumableUseResult {
        status: ConsumableUseStatus::Used,
        quantity_remaining,
    })
}

pub async fn consume_aquarium_food_pinch(
    client: &mut Client,
    user_id: Uuid,
) -> Result<ConsumableUseResult> {
    let tx = client.transaction().await?;
    let Some(item_row) = tx
        .query_opt(
            "SELECT *
             FROM marketplace_items
             WHERE sku = $1
             FOR UPDATE",
            &[&AQUARIUM_FOOD_SKU],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::NotAvailable,
            quantity_remaining: 0,
        });
    };
    let item = MarketplaceItem::from(item_row);
    if item.item_kind != COMPANION_CONSUMABLE_ITEM_KIND
        || item
            .payload
            .get("effect_kind")
            .and_then(|value| value.as_str())
            != Some("aquarium_food")
    {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::NotConsumable,
            quantity_remaining: 0,
        });
    }

    let Some(purchase_row) = tx
        .query_opt(
            "SELECT p.quantity
             FROM user_purchases p
             WHERE p.user_id = $1 AND p.item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::OutOfStock,
            quantity_remaining: 0,
        });
    };
    let quantity = purchase_row.get::<_, i32>("quantity");
    if quantity <= 0 {
        tx.commit().await?;
        return Ok(ConsumableUseResult {
            status: ConsumableUseStatus::OutOfStock,
            quantity_remaining: 0,
        });
    }

    let quantity_remaining = quantity - 1;
    tx.execute(
        "UPDATE user_purchases
         SET quantity = $3,
             active_quantity = LEAST(active_quantity, $3),
             updated = current_timestamp
         WHERE user_id = $1 AND item_id = $2",
        &[&user_id, &item.id, &quantity_remaining],
    )
    .await?;
    tx.execute(
        "INSERT INTO user_aquarium_care (user_id, last_fed)
         VALUES ($1, current_timestamp)
         ON CONFLICT (user_id) DO UPDATE
         SET last_fed = EXCLUDED.last_fed,
             updated = current_timestamp",
        &[&user_id],
    )
    .await?;
    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;
    tx.commit().await?;
    Ok(ConsumableUseResult {
        status: ConsumableUseStatus::Used,
        quantity_remaining,
    })
}

pub async fn aquarium_is_hungry(client: &Client, user_id: Uuid) -> Result<bool> {
    let cutoff = Utc::now() - Duration::hours(AQUARIUM_HUNGER_AFTER_HOURS);
    let Some(row) = client
        .query_opt(
            "SELECT c.last_fed
             FROM (
                 SELECT 1
                 FROM user_purchases p
                 JOIN marketplace_items i ON i.id = p.item_id
                 WHERE p.user_id = $1 AND i.sku = $2
                 LIMIT 1
             ) aquarium_purchase
             LEFT JOIN user_aquarium_care c ON c.user_id = $1",
            &[&user_id, &AQUARIUM_SKU],
        )
        .await?
    else {
        return Ok(false);
    };
    let last_fed: Option<DateTime<Utc>> = row.get("last_fed");
    Ok(last_fed.is_none_or(|time| time <= cutoff))
}

pub async fn equip_owned_item_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
) -> Result<Option<EquipResult>> {
    let tx = client.transaction().await?;
    let Some(row) = tx
        .query_opt(
            "SELECT i.*
             FROM marketplace_items i
             WHERE i.sku = $1",
            &[&sku],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };
    let item = MarketplaceItem::from(row);

    let Some(slot) = item.slot.clone() else {
        tx.commit().await?;
        return Ok(Some(EquipResult {
            status: EquipStatus::NotEquippable,
            item,
        }));
    };

    let Some(purchase_row) = tx
        .query_opt(
            "SELECT equipped_slot
             FROM user_purchases
             WHERE user_id = $1 AND item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(Some(EquipResult {
            status: EquipStatus::NotOwned,
            item,
        }));
    };

    let already_equipped = purchase_row
        .get::<_, Option<String>>("equipped_slot")
        .as_deref()
        == Some(slot.as_str());
    if already_equipped {
        tx.commit().await?;
        return Ok(Some(EquipResult {
            status: EquipStatus::AlreadyEquipped,
            item,
        }));
    }

    equip_purchase_in_tx(&tx, user_id, item.id, &slot).await?;
    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;

    tx.commit().await?;
    Ok(Some(EquipResult {
        status: EquipStatus::Equipped,
        item,
    }))
}

pub async fn unequip_slot(client: &mut Client, user_id: Uuid, slot: &str) -> Result<bool> {
    let tx = client.transaction().await?;
    let updated = tx
        .execute(
            "UPDATE user_purchases
             SET equipped_slot = NULL, updated = current_timestamp
             WHERE user_id = $1 AND equipped_slot = $2",
            &[&user_id, &slot],
        )
        .await?;

    if updated > 0 {
        let payload = user_id.to_string();
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_USER_CHANGED_CHANNEL, &payload],
        )
        .await?;
    }

    tx.commit().await?;
    Ok(updated > 0)
}

/// Active aquarium creatures `(creature_name, count)` a user is currently
/// displaying. Mirrors `ShopState::active_aquarium_fish` but reads from the
/// database for an arbitrary user, so profile views can render someone else's
/// tank.
pub async fn active_aquarium_fish_for_user(
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
               AND i.item_kind = $2
               AND p.active_quantity > 0
               AND i.payload->>'creature' IS NOT NULL
             ORDER BY creature",
            &[&user_id, &AQUARIUM_FISH_ITEM_KIND],
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

/// Whether the user has Dynamic Bonsai equipped in the `bonsai_variant` slot.
/// Same rule the chat badge uses, exposed for the profile view.
pub async fn is_dynamic_bonsai_selected(client: &Client, user_id: Uuid) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (
                 SELECT 1
                 FROM user_purchases p
                 JOIN marketplace_items i ON i.id = p.item_id
                 WHERE p.user_id = $1
                   AND p.equipped_slot = $2
                   AND i.sku = $3
             ) AS selected",
            &[&user_id, &BONSAI_VARIANT_SLOT, &DYNAMIC_BONSAI_SKU],
        )
        .await?;
    Ok(row.get("selected"))
}

async fn aquarium_fish_active_quantity_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
) -> Result<i32> {
    let row = tx
        .query_one(
            "SELECT COALESCE(SUM(p.active_quantity), 0)::INT AS total
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.item_kind = $2",
            &[&user_id, &AQUARIUM_FISH_ITEM_KIND],
        )
        .await?;
    Ok(row.get("total"))
}

fn is_repeatable_purchase_item(item: &MarketplaceItem) -> bool {
    matches!(
        item.item_kind.as_str(),
        AQUARIUM_FISH_ITEM_KIND | CHAT_CONSUMABLE_ITEM_KIND | COMPANION_CONSUMABLE_ITEM_KIND
    )
}

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
    let requires_room = item.payload.get("target").and_then(|value| value.as_str()) == Some("room");

    if requires_room {
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
    } else {
        ShopConsumableEffect::activate_user_effect_in_tx(
            tx,
            user_id,
            effect_kind,
            &item.sku,
            duration_secs,
            item.payload.clone(),
        )
        .await?;
        Ok(false)
    }
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
                &SHOP_PURCHASE_REASON,
                &MARKETPLACE_SOURCE_KIND,
                &item.sku,
            ],
        )
        .await?;
    Ok(row.get("purchased_today"))
}

async fn lock_user_chips_in_tx(tx: &tokio_postgres::Transaction<'_>, user_id: Uuid) -> Result<i64> {
    tx.execute(
        "INSERT INTO user_chips (user_id, balance)
         VALUES ($1, $2)
         ON CONFLICT (user_id) DO NOTHING",
        &[&user_id, &INITIAL_CHIP_BALANCE],
    )
    .await?;
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
