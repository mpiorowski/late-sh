use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ShopConsumableEffect {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub user_id: Uuid,
    pub room_id: Option<Uuid>,
    pub effect_kind: String,
    pub source_sku: String,
    pub payload: Value,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub active: bool,
}

impl From<tokio_postgres::Row> for ShopConsumableEffect {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            user_id: row.get("user_id"),
            room_id: row.get("room_id"),
            effect_kind: row.get("effect_kind"),
            source_sku: row.get("source_sku"),
            payload: row.get("payload"),
            starts_at: row.get("starts_at"),
            ends_at: row.get("ends_at"),
            active: row.get("active"),
        }
    }
}

impl ShopConsumableEffect {
    pub async fn activate_room_effect(
        client: &Client,
        user_id: Uuid,
        room_id: Uuid,
        effect_kind: &str,
        source_sku: &str,
        duration_secs: i64,
        payload: Value,
    ) -> Result<Self> {
        let duration_secs = duration_secs.max(1);
        client
            .execute(
                "UPDATE shop_consumable_effects
                 SET active = false, updated = current_timestamp
                 WHERE room_id = $1
                   AND effect_kind = $2
                   AND active = true
                   AND ends_at > current_timestamp",
                &[&room_id, &effect_kind],
            )
            .await?;

        let ends_at = Utc::now() + Duration::seconds(duration_secs);
        let row = client
            .query_one(
                "INSERT INTO shop_consumable_effects
                    (user_id, room_id, effect_kind, source_sku, payload, ends_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING *",
                &[
                    &user_id,
                    &room_id,
                    &effect_kind,
                    &source_sku,
                    &payload,
                    &ends_at,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn activate_user_effect(
        client: &Client,
        user_id: Uuid,
        effect_kind: &str,
        source_sku: &str,
        duration_secs: i64,
        payload: Value,
    ) -> Result<Self> {
        let duration_secs = duration_secs.max(1);
        client
            .execute(
                "UPDATE shop_consumable_effects
                 SET active = false, updated = current_timestamp
                 WHERE user_id = $1
                   AND room_id IS NULL
                   AND effect_kind = $2
                   AND active = true
                   AND ends_at > current_timestamp",
                &[&user_id, &effect_kind],
            )
            .await?;

        let ends_at = Utc::now() + Duration::seconds(duration_secs);
        let row = client
            .query_one(
                "INSERT INTO shop_consumable_effects
                    (user_id, room_id, effect_kind, source_sku, payload, ends_at)
                 VALUES ($1, NULL, $2, $3, $4, $5)
                 RETURNING *",
                &[&user_id, &effect_kind, &source_sku, &payload, &ends_at],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn active_room_effects(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM shop_consumable_effects
                 WHERE room_id IS NOT NULL
                   AND active = true
                   AND ends_at > current_timestamp
                 ORDER BY room_id, ends_at DESC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn active_user_effect_exists(
        client: &Client,
        user_id: Uuid,
        effect_kind: &str,
    ) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS (
                     SELECT 1
                     FROM shop_consumable_effects
                     WHERE user_id = $1
                       AND room_id IS NULL
                       AND effect_kind = $2
                       AND active = true
                       AND ends_at > current_timestamp
                 ) AS active",
                &[&user_id, &effect_kind],
            )
            .await?;
        Ok(row.get("active"))
    }
}
