use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

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

    pub async fn add_growth(client: &Client, user_id: Uuid, points: i32) -> Result<()> {
        client
            .execute(
                "UPDATE bonsai_trees SET growth_points = growth_points + $2, updated = current_timestamp WHERE user_id = $1",
                &[&user_id, &points],
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
