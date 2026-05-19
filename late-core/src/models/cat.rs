use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "cat_companions";
    user_field = user_id;
    params = CatCompanionParams;
    struct CatCompanion {
        @data
        pub user_id: Uuid,
        pub last_fed: Option<DateTime<Utc>>,
        pub last_watered: Option<DateTime<Utc>>,
        pub last_played: Option<DateTime<Utc>>,
        pub last_groomed: Option<DateTime<Utc>>,
        pub last_treated: Option<DateTime<Utc>>,
    }
}

impl CatCompanion {
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO cat_companions (user_id) VALUES ($1)
                 ON CONFLICT (user_id) DO UPDATE SET updated = cat_companions.updated
                 RETURNING *",
                &[&user_id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn touch_fed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_fed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_watered(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_watered = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_played(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_played = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_groomed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_groomed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_treated(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE cat_companions SET last_treated = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }
}
