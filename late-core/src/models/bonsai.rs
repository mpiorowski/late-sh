use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

crate::user_scoped_model! {
    table = "bonsai_trees";
    user_field = user_id;
    params = TreeParams;
    struct Tree {
        @data
        pub user_id: Uuid,
        pub seed: i64,
        pub last_watered: Option<NaiveDate>,
        pub is_alive: bool,
        pub vigor: i32,
        pub water_stress: i32,
        pub last_simulated_date: NaiveDate,
        pub branch_graph: serde_json::Value,
        pub selected_branch_id: Option<i32>,
        pub mode: String,
        pub badge_glyph: String,
        pub planted_at: DateTime<Utc>,
        pub state_revision: i64,
    }
}

/// Carries the owner's user id as its payload: the row is re-read by
/// whoever cares, never trusted from a serialized copy.
pub const BONSAI_CHANGED_CHANNEL: &str = "bonsai_changed";

pub async fn listen_for_bonsai_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {BONSAI_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// Everything `Tree::store` writes. No revision: the row owns that.
#[derive(Clone, Debug)]
pub struct TreeWrite {
    pub user_id: Uuid,
    pub seed: i64,
    pub last_watered: Option<NaiveDate>,
    pub is_alive: bool,
    pub vigor: i32,
    pub water_stress: i32,
    pub last_simulated_date: NaiveDate,
    pub branch_graph: serde_json::Value,
    pub selected_branch_id: Option<i32>,
    pub mode: String,
    pub badge_glyph: String,
    pub planted_at: DateTime<Utc>,
}

impl Tree {
    /// Load the user's tree, planting a fresh one from `branch_graph` when
    /// none exists yet. An existing row is returned untouched.
    pub async fn ensure(
        client: &impl GenericClient,
        user_id: Uuid,
        seed: i64,
        today: NaiveDate,
        branch_graph: serde_json::Value,
        badge_glyph: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO bonsai_trees
                    (user_id, seed, last_simulated_date, branch_graph, badge_glyph)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (user_id) DO UPDATE SET updated = bonsai_trees.updated
                 RETURNING *",
                &[&user_id, &seed, &today, &branch_graph, &badge_glyph],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Take the user's row lock for the rest of the transaction. Every
    /// write to a tree happens behind this lock: the caller reads the row,
    /// applies one rule to it, and stores the result, so two sessions (or
    /// two replicas) acting at once run one after the other on the same
    /// truth instead of overwriting each other from private copies.
    pub async fn lock(client: &impl GenericClient, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "SELECT * FROM bonsai_trees WHERE user_id = $1 FOR UPDATE",
                &[&user_id],
            )
            .await
            .context("locking bonsai tree")?;
        Ok(Self::from(row))
    }

    /// Write the whole tree back. Call it only while holding `lock`.
    /// `state_revision` is owned here: it counts stored writes, so a session
    /// holding two copies of the row can tell which one is newer.
    pub async fn store(client: &impl GenericClient, write: TreeWrite) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE bonsai_trees
                 SET seed = $2,
                     last_watered = $3,
                     is_alive = $4,
                     vigor = $5,
                     water_stress = $6,
                     last_simulated_date = $7,
                     branch_graph = $8,
                     selected_branch_id = $9,
                     mode = $10,
                     badge_glyph = $11,
                     planted_at = $12,
                     state_revision = state_revision + 1,
                     updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[
                    &write.user_id,
                    &write.seed,
                    &write.last_watered,
                    &write.is_alive,
                    &write.vigor,
                    &write.water_stress,
                    &write.last_simulated_date,
                    &write.branch_graph,
                    &write.selected_branch_id,
                    &write.mode,
                    &write.badge_glyph,
                    &write.planted_at,
                ],
            )
            .await
            .context("storing bonsai tree")?;
        Ok(Self::from(row))
    }

    /// Tell every replica this user's tree moved. Sent inside the writing
    /// transaction, so it is delivered on commit and never for a rollback.
    pub async fn notify_changed(client: &impl GenericClient, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "SELECT pg_notify($1, $2)",
                &[&BONSAI_CHANGED_CHANNEL, &user_id.to_string()],
            )
            .await
            .context("notifying bonsai change")?;
        Ok(())
    }
}
