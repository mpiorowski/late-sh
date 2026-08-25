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

    pub async fn activate_room_effect_in_tx(
        tx: &tokio_postgres::Transaction<'_>,
        user_id: Uuid,
        room_id: Uuid,
        effect_kind: &str,
        source_sku: &str,
        duration_secs: i64,
        payload: Value,
    ) -> Result<Self> {
        let duration_secs = duration_secs.max(1);
        tx.execute(
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
        let row = tx
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

    /// Activate a user-scoped effect (`room_id IS NULL`), deactivating every
    /// prior active effect of the same kind for the user in the same
    /// transaction. Keyed on `effect_kind`, not sku, so rebuying any username
    /// effect replaces the previous one and resets its clock. Expired rows
    /// are deactivated too, keeping them out of the active partial index.
    pub async fn activate_user_effect_in_tx(
        tx: &tokio_postgres::Transaction<'_>,
        user_id: Uuid,
        effect_kind: &str,
        source_sku: &str,
        duration_secs: i64,
        payload: Value,
    ) -> Result<Self> {
        let duration_secs = duration_secs.max(1);
        tx.execute(
            "UPDATE shop_consumable_effects
             SET active = false, updated = current_timestamp
             WHERE user_id = $1
               AND effect_kind = $2
               AND room_id IS NULL
               AND active = true",
            &[&user_id, &effect_kind],
        )
        .await?;

        let ends_at = Utc::now() + Duration::seconds(duration_secs);
        let row = tx
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

    /// Activate-or-extend a user-scoped effect (`room_id IS NULL`). Unlike
    /// `activate_user_effect_in_tx`, which always resets the clock to
    /// `duration_secs` from now, this extends any live expiry of the same
    /// kind by `duration_secs` instead of restarting it, so a stacking
    /// consumable (the Bonsai Decay Shield) never discards time the player
    /// already paid for. Every prior row is deactivated in the same
    /// transaction, leaving exactly one active row per (user, effect_kind,
    /// room_id IS NULL).
    pub async fn extend_user_effect_in_tx(
        tx: &tokio_postgres::Transaction<'_>,
        user_id: Uuid,
        effect_kind: &str,
        source_sku: &str,
        duration_secs: i64,
        payload: Value,
    ) -> Result<Self> {
        let duration_secs = duration_secs.max(1);
        let now = Utc::now();

        // Nothing in the schema enforces one active user-scoped row per kind
        // (migrations 071 and 112 index these columns, they do not constrain
        // them), so read every row this deactivated rather than assuming a
        // single one: two concurrent purchases racing here would otherwise
        // wedge the item, failing every later rebuy on an unexpected row
        // count.
        let deactivated = tx
            .query(
                "UPDATE shop_consumable_effects
                 SET active = false, updated = current_timestamp
                 WHERE user_id = $1
                   AND effect_kind = $2
                   AND room_id IS NULL
                   AND active = true
                 RETURNING starts_at, ends_at",
                &[&user_id, &effect_kind],
            )
            .await?;

        // A row can sit in the table with `active = true` long after its
        // `ends_at` has passed (expiry is enforced by the active-effect
        // queries filtering on `ends_at`, not by a background job), so only
        // treat a prior row as "still live" when its own expiry hasn't
        // passed yet. When one is still live, the new row's `starts_at`
        // carries forward the prior activation instead of resetting to now,
        // so a mid-window rebuy doesn't erase the protection credit for days
        // already covered by the row it replaces (see
        // `BonsaiDecayProtection::protected_days_between`). When every prior
        // row had already lapsed, this is a fresh window starting now: the
        // gap during which the shield was not live must not count as
        // protected.
        let still_live = deactivated
            .iter()
            .map(|row| {
                (
                    row.get::<_, DateTime<Utc>>("starts_at"),
                    row.get::<_, DateTime<Utc>>("ends_at"),
                )
            })
            .filter(|(_, ends_at)| *ends_at > now)
            .max_by_key(|(_, ends_at)| *ends_at);
        let starts_at = still_live.map(|(starts_at, _)| starts_at).unwrap_or(now);
        let base = still_live.map(|(_, ends_at)| ends_at).unwrap_or(now);
        let ends_at = base + Duration::seconds(duration_secs);

        let row = tx
            .query_one(
                "INSERT INTO shop_consumable_effects
                    (user_id, room_id, effect_kind, source_sku, payload, starts_at, ends_at)
                 VALUES ($1, NULL, $2, $3, $4, $5, $6)
                 RETURNING *",
                &[
                    &user_id,
                    &effect_kind,
                    &source_sku,
                    &payload,
                    &starts_at,
                    &ends_at,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// All live user-scoped effects of one kind, for seeding the in-process
    /// flair directory at startup.
    pub async fn active_user_effects(client: &Client, effect_kind: &str) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM shop_consumable_effects
                 WHERE room_id IS NULL
                   AND effect_kind = $1
                   AND active = true
                   AND ends_at > current_timestamp
                 ORDER BY user_id, ends_at DESC",
                &[&effect_kind],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// The single live user-scoped effect of one kind for one user, if any.
    pub async fn active_user_effect_for_user(
        client: &Client,
        user_id: Uuid,
        effect_kind: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM shop_consumable_effects
                 WHERE user_id = $1
                   AND room_id IS NULL
                   AND effect_kind = $2
                   AND active = true
                   AND ends_at > current_timestamp
                 ORDER BY ends_at DESC
                 LIMIT 1",
                &[&user_id, &effect_kind],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Every live user-scoped effect of the given kinds for one user, newest
    /// expiry first per kind. One query for the whole set, so a snapshot that
    /// needs the username effect, the badge rental, the flag rental and the
    /// title does not pay four round trips for them.
    pub async fn active_user_effects_for_user(
        client: &Client,
        user_id: Uuid,
        effect_kinds: &[&str],
    ) -> Result<Vec<Self>> {
        if effect_kinds.is_empty() {
            return Ok(Vec::new());
        }
        let kinds: Vec<String> = effect_kinds.iter().map(|kind| kind.to_string()).collect();
        let rows = client
            .query(
                "SELECT *
                 FROM shop_consumable_effects
                 WHERE user_id = $1
                   AND room_id IS NULL
                   AND effect_kind = ANY($2)
                   AND active = true
                   AND ends_at > current_timestamp
                 ORDER BY effect_kind, ends_at DESC",
                &[&user_id, &kinds],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
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
}
