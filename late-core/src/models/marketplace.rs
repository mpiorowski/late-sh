use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::INITIAL_CHIP_BALANCE;

pub const CAT_COMPANION_SKU: &str = "cat_companion";
pub const AQUARIUM_SKU: &str = "aquarium";
pub const AQUARIUM_FISH_ITEM_KIND: &str = "aquarium_fish";
pub const AQUARIUM_MAX_FISH: i32 = 20;
pub const CHAT_BADGE_SLOT: &str = "chat_badge";
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
    FishLimitReached,
    RequiresAquarium,
}

#[derive(Debug, Clone)]
pub struct PurchaseResult {
    pub status: PurchaseStatus,
    pub item: MarketplaceItem,
    pub balance: i64,
    pub quantity: i32,
    pub active_quantity: i32,
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

pub async fn purchase_durable_item_by_sku(
    client: &mut Client,
    user_id: Uuid,
    sku: &str,
) -> Result<Option<PurchaseResult>> {
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
        return Ok(None);
    };
    let item = MarketplaceItem::from(item_row);
    let is_aquarium_fish = item.item_kind == AQUARIUM_FISH_ITEM_KIND;

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
            return Ok(Some(PurchaseResult {
                status: PurchaseStatus::RequiresAquarium,
                item,
                balance: 0,
                quantity: 0,
                active_quantity: 0,
            }));
        }
    }

    tx.execute(
        "INSERT INTO user_chips (user_id, balance)
         VALUES ($1, $2)
         ON CONFLICT (user_id) DO NOTHING",
        &[&user_id, &INITIAL_CHIP_BALANCE],
    )
    .await?;

    let balance_row = tx
        .query_one(
            "SELECT balance
             FROM user_chips
             WHERE user_id = $1
             FOR UPDATE",
            &[&user_id],
        )
        .await?;
    let balance: i64 = balance_row.get("balance");

    if let Some(existing) = existing {
        let quantity = existing.get::<_, i32>("quantity");
        let active_quantity = existing.get::<_, i32>("active_quantity");
        if !is_aquarium_fish {
            tx.commit().await?;
            return Ok(Some(PurchaseResult {
                status: PurchaseStatus::AlreadyOwned,
                item,
                balance,
                quantity,
                active_quantity,
            }));
        }

        let fish_total = aquarium_fish_quantity_in_tx(&tx, user_id).await?;
        if fish_total >= AQUARIUM_MAX_FISH {
            tx.commit().await?;
            return Ok(Some(PurchaseResult {
                status: PurchaseStatus::FishLimitReached,
                item,
                balance,
                quantity,
                active_quantity,
            }));
        }

        if balance < item.price_chips {
            tx.commit().await?;
            return Ok(Some(PurchaseResult {
                status: PurchaseStatus::InsufficientFunds,
                item,
                balance,
                quantity,
                active_quantity,
            }));
        }

        let new_balance = balance - item.price_chips;
        let active_total = aquarium_fish_active_quantity_in_tx(&tx, user_id).await?;
        let new_active_quantity = if active_total < AQUARIUM_MAX_FISH {
            active_quantity + 1
        } else {
            active_quantity
        };
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
                 active_quantity = $3,
                 purchased_price_chips = $4,
                 updated = current_timestamp
             WHERE user_id = $1 AND item_id = $2",
            &[&user_id, &item.id, &new_active_quantity, &item.price_chips],
        )
        .await?;
        let payload = user_id.to_string();
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&SHOP_USER_CHANGED_CHANNEL, &payload],
        )
        .await?;
        tx.commit().await?;
        return Ok(Some(PurchaseResult {
            status: PurchaseStatus::QuantityAdded,
            item,
            balance: new_balance,
            quantity: quantity + 1,
            active_quantity: new_active_quantity,
        }));
    }

    if is_aquarium_fish {
        let fish_total = aquarium_fish_quantity_in_tx(&tx, user_id).await?;
        if fish_total >= AQUARIUM_MAX_FISH {
            tx.commit().await?;
            return Ok(Some(PurchaseResult {
                status: PurchaseStatus::FishLimitReached,
                item,
                balance,
                quantity: 0,
                active_quantity: 0,
            }));
        }
    }

    if balance < item.price_chips {
        tx.commit().await?;
        return Ok(Some(PurchaseResult {
            status: PurchaseStatus::InsufficientFunds,
            item,
            balance,
            quantity: 0,
            active_quantity: 0,
        }));
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

    let active_quantity = if is_aquarium_fish { 1 } else { 0 };
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

    let payload = user_id.to_string();
    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&SHOP_USER_CHANGED_CHANNEL, &payload],
    )
    .await?;

    tx.commit().await?;
    Ok(Some(PurchaseResult {
        status: PurchaseStatus::Purchased,
        item,
        balance: new_balance,
        quantity: 1,
        active_quantity,
    }))
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
    if delta > 0 {
        let active_total = aquarium_fish_active_quantity_in_tx(&tx, user_id).await?;
        if active_total >= AQUARIUM_MAX_FISH {
            tx.commit().await?;
            return Ok(Some(FishActiveResult {
                status: FishActiveStatus::TankFull,
                item,
                quantity,
                active_quantity,
            }));
        }
    }

    let next_active = active_quantity.saturating_add(delta).clamp(0, quantity);
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

async fn aquarium_fish_quantity_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
) -> Result<i32> {
    let row = tx
        .query_one(
            "SELECT COALESCE(SUM(p.quantity), 0)::INT AS total
             FROM user_purchases p
             JOIN marketplace_items i ON i.id = p.item_id
             WHERE p.user_id = $1 AND i.item_kind = $2",
            &[&user_id, &AQUARIUM_FISH_ITEM_KIND],
        )
        .await?;
    Ok(row.get("total"))
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
