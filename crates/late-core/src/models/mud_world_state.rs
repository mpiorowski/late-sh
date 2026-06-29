// Persistent global MUD world state.
//
// Door/MUD games own the opaque JSON shape. This model only provides keyed load
// and upsert operations for one shared world runtime row.

use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;

crate::model! {
    table = "mud_world_states";
    params = MudWorldStateParams;
    struct MudWorldState {
        @data
        pub world_key: String,
        pub data: Value,
    }
}

impl MudWorldState {
    pub async fn load(client: &Client, world_key: &str) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT data FROM mud_world_states WHERE world_key = $1",
                &[&world_key],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, Value>("data")))
    }

    pub async fn save(client: &Client, world_key: &str, data: Value) -> Result<()> {
        client
            .execute(
                "INSERT INTO mud_world_states (world_key, data)
                 VALUES ($1, $2)
                 ON CONFLICT (world_key) DO UPDATE
                 SET data = EXCLUDED.data,
                     updated = current_timestamp",
                &[&world_key, &data],
            )
            .await?;
        Ok(())
    }
}
