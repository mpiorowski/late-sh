//! The aquarium's care row: one timestamp per owner, the day's feeding.
//! Hunger is not stored; it is "not fed today (UTC)", read off `last_fed`.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

pub struct AquariumCare;

impl AquariumCare {
    /// When the tank was last fed, `None` for a tank never fed.
    pub async fn last_fed(client: &Client, user_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT last_fed FROM user_aquarium_care WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|row| row.get("last_fed")))
    }

    /// Stamp today's feeding and report whether this call was the first of
    /// the UTC day. The only witness the daily chip bonus needs, atomic no
    /// matter how many sessions or presses race for it. Takes a
    /// `GenericClient` so the credit can share its transaction.
    pub async fn feed_day(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "INSERT INTO user_aquarium_care (user_id, last_fed)
                 VALUES ($1, current_timestamp)
                 ON CONFLICT (user_id) DO UPDATE
                 SET last_fed = current_timestamp,
                     updated = current_timestamp
                 WHERE (user_aquarium_care.last_fed AT TIME ZONE 'UTC')::date IS DISTINCT FROM $2
                 RETURNING user_id",
                &[&user_id, &today],
            )
            .await?;
        Ok(row.is_some())
    }
}
