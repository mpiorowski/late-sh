// Ingestion resume points: how far into each door host log file late-ssh has
// read. `next_offset` is the byte offset of the first unread line, exactly
// what the stats session's frames carry, and is written in the same
// transaction as the fact insert so a crash can never skip a line (at worst
// it replays one, which the fact tables' unique keys absorb). A missing row
// means start at 0 and backfill the history already on the PVC.

use std::collections::HashMap;

use anyhow::Result;
use deadpool_postgres::GenericClient;

pub struct DoorLogCursor;

impl DoorLogCursor {
    /// Every stored cursor for one game, keyed by file id.
    pub async fn all_for_game(
        client: &impl GenericClient,
        game: &str,
    ) -> Result<HashMap<String, i64>> {
        let rows = client
            .query(
                "SELECT file, next_offset FROM door_log_cursors WHERE game = $1",
                &[&game],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("file"), row.get("next_offset")))
            .collect())
    }

    pub async fn upsert(
        client: &impl GenericClient,
        game: &str,
        file: &str,
        next_offset: i64,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO door_log_cursors (game, file, next_offset)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (game, file) DO UPDATE SET
                   next_offset = EXCLUDED.next_offset,
                   updated = current_timestamp",
                &[&game, &file, &next_offset],
            )
            .await?;
        Ok(())
    }
}
