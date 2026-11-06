//! Who was on the YouTube source on which UTC day (`youtube_listens`,
//! migration 264): every read and write of the table.
//!
//! A row is one account on YouTube on one day, the booth's twin of
//! `radio_listen.rs`. The audio service's roll call
//! (`late-ssh/src/app/audio/svc.rs`) writes one for every online account on
//! the YouTube source, on every replica, and the same account again is a
//! no-op, so a day's rows count listeners, never minutes. The paper's
//! YESTERDAY column reads a day's count.

use anyhow::Result;
use chrono::NaiveDate;
use deadpool_postgres::GenericClient;
use uuid::Uuid;

pub struct YoutubeListen;

impl YoutubeListen {
    /// Write `user_id` down as on YouTube on `day`. Returns whether the row
    /// is new; a listener already written for the day changes nothing.
    pub async fn record(
        client: &impl GenericClient,
        user_id: Uuid,
        day: NaiveDate,
    ) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO youtube_listens (user_id, day)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id, day) DO NOTHING",
                &[&user_id, &day],
            )
            .await?;
        Ok(inserted == 1)
    }

    /// How many accounts were on YouTube on `day`. Zero on a day nobody was.
    pub async fn listeners_on(client: &impl GenericClient, day: NaiveDate) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT count(*)::bigint AS listeners
                 FROM youtube_listens
                 WHERE day = $1",
                &[&day],
            )
            .await?;
        Ok(row.get("listeners"))
    }
}
