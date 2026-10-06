// Persistent Lateania (MUD) character storage.
//
// Up to a handful of rows per user (one per character slot), each holding a
// schema-versioned JSON blob. The MUD game owns the blob's shape; this model
// only loads and upserts it. Keeping the character as opaque JSON lets the
// game add fields (new stats, inventory, quest flags) without a migration
// each time.

use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "mud_characters";
    params = MudCharacterParams;
    struct MudCharacter {
        @data
        pub user_id: Uuid,
        pub slot: i16,
        pub data: Value,
    }
}

impl MudCharacter {
    /// Load one character slot's saved blob, if it has one.
    pub async fn load(client: &Client, user_id: Uuid, slot: i16) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT data FROM mud_characters WHERE user_id = $1 AND slot = $2",
                &[&user_id, &slot],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, Value>("data")))
    }

    /// One slot's row id, which is what a per-character payout keys on.
    /// `delete_slot` drops the row, so a recreated character in the same slot
    /// comes back with a fresh id and its first crown is a different event
    /// from the old character's.
    pub async fn id_for_slot(client: &Client, user_id: Uuid, slot: i16) -> Result<Option<Uuid>> {
        let row = client
            .query_opt(
                "SELECT id FROM mud_characters WHERE user_id = $1 AND slot = $2",
                &[&user_id, &slot],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, Uuid>("id")))
    }

    /// Every character slot a user has saved, as (slot, blob) pairs.
    pub async fn list(client: &Client, user_id: Uuid) -> Result<Vec<(i16, Value)>> {
        let rows = client
            .query(
                "SELECT slot, data FROM mud_characters WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<_, i16>("slot"), r.get::<_, Value>("data")))
            .collect())
    }

    /// Insert or overwrite one character slot's blob.
    pub async fn save(client: &Client, user_id: Uuid, slot: i16, data: Value) -> Result<()> {
        client
            .execute(
                "INSERT INTO mud_characters (user_id, slot, data)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, slot) DO UPDATE
                 SET data = EXCLUDED.data,
                     updated = current_timestamp",
                &[&user_id, &slot, &data],
            )
            .await?;
        Ok(())
    }

    /// Delete one character slot, if present.
    pub async fn delete_slot(client: &Client, user_id: Uuid, slot: i16) -> Result<()> {
        client
            .execute(
                "DELETE FROM mud_characters WHERE user_id = $1 AND slot = $2",
                &[&user_id, &slot],
            )
            .await?;
        Ok(())
    }
}
