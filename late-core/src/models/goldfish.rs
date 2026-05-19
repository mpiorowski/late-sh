use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

pub const MAX_FRIENDS: i32 = 5;

crate::user_scoped_model! {
    table = "goldfish_bowls";
    user_field = user_id;
    params = GoldfishBowlParams;
    struct GoldfishBowl {
        @data
        pub user_id: Uuid,
        pub last_fed: Option<DateTime<Utc>>,
        pub last_decorated: Option<DateTime<Utc>>,
        pub last_lit: Option<DateTime<Utc>>,
        pub last_water_changed: Option<DateTime<Utc>>,
        pub friend_count: i32,
    }
}

impl GoldfishBowl {
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO goldfish_bowls (user_id) VALUES ($1)
                 ON CONFLICT (user_id) DO UPDATE SET updated = goldfish_bowls.updated
                 RETURNING *",
                &[&user_id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn touch_fed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE goldfish_bowls SET last_fed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_decorated(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE goldfish_bowls SET last_decorated = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_lit(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE goldfish_bowls SET last_lit = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_water_changed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE goldfish_bowls SET last_water_changed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn add_friend(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE goldfish_bowls SET friend_count = LEAST(friend_count + 1, $2), updated = current_timestamp WHERE user_id = $1",
                &[&user_id, &MAX_FRIENDS],
            )
            .await?;
        Ok(())
    }
}
