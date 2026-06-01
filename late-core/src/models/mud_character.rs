// Persistent Lateania (MUD) character storage.
//
// One row per user holding a schema-versioned JSON blob. The MUD game owns the
// blob's shape; this model only loads and upserts it. Keeping the character as
// opaque JSON lets the game add fields (new stats, inventory, quest flags)
// without a migration each time, the same trade game_rooms.runtime_state makes.

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
        pub data: Value,
    }
}

impl MudCharacter {
    /// Load a user's saved character blob, if they have one.
    pub async fn load(client: &Client, user_id: Uuid) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT data FROM mud_characters WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, Value>("data")))
    }

    /// Insert or overwrite a user's character blob.
    pub async fn save(client: &Client, user_id: Uuid, data: Value) -> Result<()> {
        client
            .execute(
                "INSERT INTO mud_characters (user_id, data)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE
                 SET data = EXCLUDED.data,
                     updated = current_timestamp",
                &[&user_id, &data],
            )
            .await?;
        Ok(())
    }
}
