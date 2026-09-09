use anyhow::Result;
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

impl Tree {
    /// Load the user's tree, planting a fresh one from `branch_graph` when
    /// none exists yet. An existing row is returned untouched.
    pub async fn ensure(
        client: &Client,
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

    /// The daily watering gate: stamps `last_watered` with `today` and
    /// reports whether this call was the one that moved it. This is the only
    /// writer of `last_watered` (`save` leaves the column alone), so the
    /// once-per-day chip bonus has one atomic witness no matter how many
    /// sessions, presses, or in-flight saves race for it. Takes a
    /// `GenericClient` so the credit can share its transaction. Deliberately
    /// does not touch `state_revision`, so the save that follows still lands.
    pub async fn water_day(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE bonsai_trees
                 SET last_watered = $2,
                     updated = current_timestamp
                 WHERE user_id = $1
                   AND last_watered IS DISTINCT FROM $2
                 RETURNING user_id",
                &[&user_id, &today],
            )
            .await?;
        Ok(row.is_some())
    }

    /// Persist the whole in-memory tree. Stale async writes lose: a row
    /// whose `state_revision` already passed the incoming one is left alone.
    /// `last_watered` is not written here: `water_day` owns that column, and
    /// a save landing out of order must never pre-empt or reopen the gate.
    pub async fn save(client: &Client, params: TreeParams) -> Result<()> {
        client
            .execute(
                "INSERT INTO bonsai_trees
                    (user_id, seed, is_alive, vigor, water_stress,
                     last_simulated_date, branch_graph, selected_branch_id, mode, badge_glyph,
                     planted_at, state_revision)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (user_id) DO UPDATE
                 SET seed = EXCLUDED.seed,
                     is_alive = EXCLUDED.is_alive,
                     vigor = EXCLUDED.vigor,
                     water_stress = EXCLUDED.water_stress,
                     last_simulated_date = EXCLUDED.last_simulated_date,
                     branch_graph = EXCLUDED.branch_graph,
                     selected_branch_id = EXCLUDED.selected_branch_id,
                     mode = EXCLUDED.mode,
                     badge_glyph = EXCLUDED.badge_glyph,
                     planted_at = EXCLUDED.planted_at,
                     state_revision = EXCLUDED.state_revision,
                     updated = current_timestamp
                 WHERE bonsai_trees.state_revision < EXCLUDED.state_revision",
                &[
                    &params.user_id,
                    &params.seed,
                    &params.is_alive,
                    &params.vigor,
                    &params.water_stress,
                    &params.last_simulated_date,
                    &params.branch_graph,
                    &params.selected_branch_id,
                    &params.mode,
                    &params.badge_glyph,
                    &params.planted_at,
                    &params.state_revision,
                ],
            )
            .await?;
        Ok(())
    }

    /// The selection cursor alone. Display state that changes on every Tab
    /// and wheel notch, so it must not cost a full graph upsert; it bumps no
    /// revision, and the next real save carries the same value anyway.
    pub async fn select_branch(
        client: &Client,
        user_id: Uuid,
        selected_branch_id: Option<i32>,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees
                 SET selected_branch_id = $2,
                     updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &selected_branch_id],
            )
            .await?;
        Ok(())
    }
}
