use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::INITIAL_CHIP_BALANCE;

pub const CAT_COMPANION_SKU: &str = "cat_companion";
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
    AlreadyOwned,
    InsufficientFunds,
}

#[derive(Debug, Clone)]
pub struct PurchaseResult {
    pub status: PurchaseStatus,
    pub item: MarketplaceItem,
    pub balance: i64,
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

    let existing = tx
        .query_opt(
            "SELECT 1
             FROM user_purchases
             WHERE user_id = $1 AND item_id = $2
             FOR UPDATE",
            &[&user_id, &item.id],
        )
        .await?;

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

    if existing.is_some() {
        tx.commit().await?;
        return Ok(Some(PurchaseResult {
            status: PurchaseStatus::AlreadyOwned,
            item,
            balance,
        }));
    }

    if balance < item.price_chips {
        tx.commit().await?;
        return Ok(Some(PurchaseResult {
            status: PurchaseStatus::InsufficientFunds,
            item,
            balance,
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

    tx.execute(
        "INSERT INTO user_purchases
            (user_id, item_id, quantity, remaining_uses, equipped_slot, purchased_price_chips)
         VALUES ($1, $2, 1, NULL, $3, $4)",
        &[&user_id, &item.id, &item.slot, &item.price_chips],
    )
    .await?;

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
    }))
}
