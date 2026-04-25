use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;

crate::model! {
    table = "artboard_snapshots";
    params = SnapshotParams;
    struct Snapshot {
        @data
        pub board_key: String,
        pub canvas: Value,
        pub provenance: Value,
    }
}

#[derive(Debug)]
pub struct SnapshotSummary {
    pub board_key: String,
    pub updated: DateTime<Utc>,
}

impl Snapshot {
    pub const MAIN_BOARD_KEY: &'static str = "main";

    pub async fn find_by_board_key(client: &Client, board_key: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_by_board_key_prefix(client: &Client, prefix: &str) -> Result<Vec<Self>> {
        let pattern = format!("{prefix}%");
        let rows = client
            .query(
                "SELECT * FROM artboard_snapshots
                 WHERE board_key LIKE $1
                 ORDER BY board_key DESC, created DESC",
                &[&pattern],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_summary_by_board_key(
        client: &Client,
        board_key: &str,
    ) -> Result<Option<SnapshotSummary>> {
        let row = client
            .query_opt(
                "SELECT board_key, updated FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(row.map(|row| SnapshotSummary {
            board_key: row.get("board_key"),
            updated: row.get("updated"),
        }))
    }

    pub async fn list_summaries_by_board_key_prefix(
        client: &Client,
        prefix: &str,
    ) -> Result<Vec<SnapshotSummary>> {
        let pattern = format!("{prefix}%");
        let rows = client
            .query(
                "SELECT board_key, updated FROM artboard_snapshots
                 WHERE board_key LIKE $1
                 ORDER BY board_key DESC, created DESC",
                &[&pattern],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| SnapshotSummary {
                board_key: row.get("board_key"),
                updated: row.get("updated"),
            })
            .collect())
    }

    pub async fn delete_by_board_key(client: &Client, board_key: &str) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(count)
    }

    pub async fn upsert(
        client: &Client,
        board_key: &str,
        canvas: Value,
        provenance: Value,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO artboard_snapshots (board_key, canvas, provenance)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (board_key) DO UPDATE
                 SET canvas = EXCLUDED.canvas,
                     provenance = EXCLUDED.provenance,
                     updated = current_timestamp
                 RETURNING *",
                &[&board_key, &canvas, &provenance],
            )
            .await?;
        Ok(Self::from(row))
    }
}
