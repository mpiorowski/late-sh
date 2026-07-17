// Persistent Legend of the Green Dragon character storage.
//
// One row per user holding a schema-versioned JSON blob. The game owns the
// blob's shape; this model only loads and upserts it. Keeping the character as
// opaque JSON lets the game add fields (new stats, inventory, run flags)
// without a migration each time — the same trade mud_characters makes.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "greendragon_characters";
    params = GreenDragonCharacterParams;
    struct GreenDragonCharacter {
        @data
        pub user_id: Uuid,
        pub data: Value,
    }
}

impl GreenDragonCharacter {
    /// Load a user's saved character blob, if they have one.
    pub async fn load(client: &Client, user_id: Uuid) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT data FROM greendragon_characters WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, Value>("data")))
    }

    /// Load every saved character for the warrior roster / Hall of Fame:
    /// `(user_id, blob, last save time)`. The game decodes the blobs and does
    /// its own sorting; the save timestamp feeds the 15-minute online window.
    pub async fn load_all(client: &Client) -> Result<Vec<(Uuid, Value, DateTime<Utc>)>> {
        let rows = client
            .query(
                "SELECT user_id, data, updated FROM greendragon_characters",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("user_id"), r.get("data"), r.get("updated")))
            .collect())
    }

    /// Load a character blob with a row lock, for a cross-player settlement
    /// transaction (a PvP engage or its aftermath). Returns the blob and its
    /// last save time; the lock holds until the caller's transaction ends,
    /// serializing concurrent attackers on the same victim.
    pub async fn load_for_update(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
    ) -> Result<Option<(Value, DateTime<Utc>)>> {
        let row = client
            .query_opt(
                "SELECT data, updated FROM greendragon_characters
                 WHERE user_id = $1 FOR UPDATE",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|r| (r.get("data"), r.get("updated"))))
    }

    /// Rewrite a character blob **without** touching `updated`. Cross-player
    /// writes (PvP flags, settlements, sleep reports) must not refresh the
    /// victim's save timestamp — it feeds the roster's presence window, and
    /// the victim didn't act.
    pub async fn update_data_keep_updated(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        data: Value,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_characters SET data = $2 WHERE user_id = $1",
                &[&user_id, &data],
            )
            .await?;
        Ok(())
    }

    /// Insert or overwrite a user's character blob.
    pub async fn save(client: &Client, user_id: Uuid, data: Value) -> Result<()> {
        client
            .execute(
                "INSERT INTO greendragon_characters (user_id, data)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE
                 SET data = EXCLUDED.data,
                     updated = current_timestamp",
                &[&user_id, &data],
            )
            .await?;
        Ok(())
    }

    /// Delete a user's saved character, if present.
    pub async fn delete_by_user_id(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "DELETE FROM greendragon_characters WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }
}
