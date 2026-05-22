use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

pub const MAX_GROWTH_POINTS: i32 = 700;

crate::user_scoped_model! {
    table = "bonsai_trees";
    user_field = user_id;
    params = TreeParams;
    struct Tree {
        @data
        pub user_id: Uuid,
        pub growth_points: i32,
        pub last_watered: Option<NaiveDate>,
        pub seed: i64,
        pub is_alive: bool,
    }
}

crate::user_scoped_model! {
    table = "bonsai_graveyard";
    user_field = user_id;
    params = GraveParams;
    struct Grave {
        @data
        pub user_id: Uuid,
        pub survived_days: i32,
        pub died_at: DateTime<Utc>,
    }
}

crate::user_scoped_model! {
    table = "bonsai_daily_care";
    user_field = user_id;
    params = DailyCareParams;
    struct DailyCare {
        @data
        pub user_id: Uuid,
        pub care_date: NaiveDate,
        pub watered: bool,
        pub cut_branch_ids: Vec<i32>,
        pub branch_goal: i32,
        pub water_penalty_applied: bool,
        pub prune_penalty_applied: bool,
    }
}

crate::user_scoped_model! {
    table = "bonsai_v2_trees";
    user_field = user_id;
    params = BonsaiV2TreeParams;
    struct BonsaiV2Tree {
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
    }
}

impl Tree {
    pub async fn ensure(client: &Client, user_id: Uuid, seed: i64) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO bonsai_trees (user_id, seed) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET updated = bonsai_trees.updated
                 RETURNING *",
                &[&user_id, &seed],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn water(client: &Client, user_id: Uuid, today: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees SET last_watered = $2, updated = current_timestamp WHERE user_id = $1",
                &[&user_id, &today],
            )
            .await?;
        Ok(())
    }

    pub async fn set_recorded_dates(
        client: &Client,
        user_id: Uuid,
        timestamp: DateTime<Utc>,
        last_watered: Option<NaiveDate>,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees
                 SET created = $2,
                     updated = $2,
                     last_watered = $3
                 WHERE user_id = $1",
                &[&user_id, &timestamp, &last_watered],
            )
            .await?;
        Ok(())
    }

    pub async fn water_and_add_growth_if_available(
        client: &Client,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE bonsai_trees
                 SET last_watered = $2,
                     growth_points = LEAST(
                         growth_points + 10 + CASE
                             WHEN $2::date - last_watered = 1 THEN 5
                             ELSE 0
                         END,
                         $3
                     ),
                     updated = current_timestamp
                 WHERE user_id = $1
                   AND is_alive = true
                   AND last_watered IS DISTINCT FROM $2
                 RETURNING user_id",
                &[&user_id, &today, &MAX_GROWTH_POINTS],
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn add_growth(client: &Client, user_id: Uuid, points: i32) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees
                 SET growth_points = LEAST(growth_points + $2, $3),
                     updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &points, &MAX_GROWTH_POINTS],
            )
            .await?;
        Ok(())
    }

    pub async fn kill(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees SET is_alive = false, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn list_all(client: &Client) -> Result<Vec<Self>> {
        let rows = client.query("SELECT * FROM bonsai_trees", &[]).await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn respawn(client: &Client, user_id: Uuid, new_seed: i64) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees SET is_alive = true, growth_points = 0, last_watered = NULL, seed = $2, created = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id, &new_seed],
            )
            .await?;
        Ok(())
    }

    pub async fn cut(client: &Client, user_id: Uuid, new_seed: i64, cost: i32) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees SET seed = $2, growth_points = GREATEST(growth_points - $3, 0), updated = current_timestamp WHERE user_id = $1",
                &[&user_id, &new_seed, &cost],
            )
            .await?;
        Ok(())
    }

    pub async fn lose_growth(client: &Client, user_id: Uuid, points: i32) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees
                 SET growth_points = GREATEST(growth_points - $2, 0),
                     updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &points],
            )
            .await?;
        Ok(())
    }
}

impl BonsaiV2Tree {
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
                "INSERT INTO bonsai_v2_trees
                    (user_id, seed, last_simulated_date, branch_graph, badge_glyph)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (user_id) DO UPDATE SET updated = bonsai_v2_trees.updated
                 RETURNING *",
                &[&user_id, &seed, &today, &branch_graph, &badge_glyph],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn save(client: &Client, params: BonsaiV2TreeParams) -> Result<()> {
        client
            .execute(
                "INSERT INTO bonsai_v2_trees
                    (user_id, seed, last_watered, is_alive, vigor, water_stress,
                     last_simulated_date, branch_graph, selected_branch_id, mode, badge_glyph)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT (user_id) DO UPDATE
                 SET seed = EXCLUDED.seed,
                     last_watered = EXCLUDED.last_watered,
                     is_alive = EXCLUDED.is_alive,
                     vigor = EXCLUDED.vigor,
                     water_stress = EXCLUDED.water_stress,
                     last_simulated_date = EXCLUDED.last_simulated_date,
                     branch_graph = EXCLUDED.branch_graph,
                     selected_branch_id = EXCLUDED.selected_branch_id,
                     mode = EXCLUDED.mode,
                     badge_glyph = EXCLUDED.badge_glyph,
                     updated = current_timestamp",
                &[
                    &params.user_id,
                    &params.seed,
                    &params.last_watered,
                    &params.is_alive,
                    &params.vigor,
                    &params.water_stress,
                    &params.last_simulated_date,
                    &params.branch_graph,
                    &params.selected_branch_id,
                    &params.mode,
                    &params.badge_glyph,
                ],
            )
            .await?;
        Ok(())
    }
}

impl Grave {
    pub async fn record(client: &Client, user_id: Uuid, survived_days: i32) -> Result<()> {
        client
            .execute(
                "INSERT INTO bonsai_graveyard (user_id, survived_days) VALUES ($1, $2)",
                &[&user_id, &survived_days],
            )
            .await?;
        Ok(())
    }

    pub async fn list_by_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM bonsai_graveyard WHERE user_id = $1 ORDER BY died_at DESC LIMIT 10",
                &[&user_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}

impl DailyCare {
    pub async fn ensure(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
        branch_goal: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO bonsai_daily_care (user_id, care_date, branch_goal)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, care_date) DO UPDATE
                 SET branch_goal = CASE
                         WHEN bonsai_daily_care.branch_goal <= 0 THEN EXCLUDED.branch_goal
                         ELSE bonsai_daily_care.branch_goal
                     END,
                     updated = bonsai_daily_care.updated
                 RETURNING *",
                &[&user_id, &care_date, &branch_goal],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn mark_watered(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE bonsai_daily_care
                 SET watered = true, updated = current_timestamp
                 WHERE user_id = $1 AND care_date = $2 AND watered = false
                 RETURNING user_id",
                &[&user_id, &care_date],
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn add_cut_branch(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
        branch_id: i32,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_daily_care
                 SET cut_branch_ids = CASE
                         WHEN $3 = ANY(cut_branch_ids) THEN cut_branch_ids
                         ELSE array_append(cut_branch_ids, $3)
                     END,
                     updated = current_timestamp
                 WHERE user_id = $1 AND care_date = $2",
                &[&user_id, &care_date, &branch_id],
            )
            .await?;
        Ok(())
    }

    pub async fn clear_cut_branches(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_daily_care
                 SET cut_branch_ids = '{}',
                     updated = current_timestamp
                 WHERE user_id = $1 AND care_date = $2",
                &[&user_id, &care_date],
            )
            .await?;
        Ok(())
    }

    pub async fn reset_for_respawn(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
        branch_goal: i32,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_daily_care
                 SET watered = false,
                     cut_branch_ids = '{}',
                     branch_goal = $3,
                     water_penalty_applied = false,
                     prune_penalty_applied = false,
                     updated = current_timestamp
                 WHERE user_id = $1 AND care_date = $2",
                &[&user_id, &care_date, &branch_goal],
            )
            .await?;
        Ok(())
    }

    pub async fn unapplied_before(
        client: &Client,
        user_id: Uuid,
        before: NaiveDate,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM bonsai_daily_care
                 WHERE user_id = $1
                   AND care_date < $2
                   AND (water_penalty_applied = false OR prune_penalty_applied = false)
                 ORDER BY care_date ASC
                 LIMIT 14",
                &[&user_id, &before],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn mark_penalties_applied(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
        water: bool,
        prune: bool,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_daily_care
                 SET water_penalty_applied = water_penalty_applied OR $3,
                     prune_penalty_applied = prune_penalty_applied OR $4,
                     updated = current_timestamp
                 WHERE user_id = $1 AND care_date = $2",
                &[&user_id, &care_date, &water, &prune],
            )
            .await?;
        Ok(())
    }
}
