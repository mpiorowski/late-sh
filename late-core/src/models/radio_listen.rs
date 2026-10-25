//! Who listened to which radio station on which UTC day (`radio_listens`,
//! migration 259): every read and write of the table.
//!
//! A row is one account heard on one station on one day. The audio
//! service's roll call (`late-ssh/src/app/audio/svc.rs`) writes one for
//! every paired client on the radio source, on every replica, and the same
//! listener again is a no-op, so a day's rows count listeners, never
//! minutes. The paper's YESTERDAY column reads a day's stations by
//! listener count.

use anyhow::Result;
use chrono::NaiveDate;
use deadpool_postgres::GenericClient;
use uuid::Uuid;

use crate::radio::RadioStation;

/// One station's listeners on one day. `station` is the catalogue key as
/// stored, which a station retired from the catalogue since still has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationListeners {
    pub station: String,
    pub listeners: i64,
}

pub struct RadioListen;

impl RadioListen {
    /// Write `user_id` down as heard on `station` on `day`. Returns whether
    /// the row is new; a listener already written for the day changes
    /// nothing.
    pub async fn record(
        client: &impl GenericClient,
        user_id: Uuid,
        station: RadioStation,
        day: NaiveDate,
    ) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO radio_listens (user_id, station, day)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, station, day) DO NOTHING",
                &[&user_id, &station.as_str(), &day],
            )
            .await?;
        Ok(inserted == 1)
    }

    /// The stations heard on `day`, most listeners first (a tie by key),
    /// at most `limit` of them. Empty on a day nobody was on the radio.
    pub async fn top_stations_on(
        client: &impl GenericClient,
        day: NaiveDate,
        limit: i64,
    ) -> Result<Vec<StationListeners>> {
        let rows = client
            .query(
                "SELECT station, count(*)::bigint AS listeners
                 FROM radio_listens
                 WHERE day = $1
                 GROUP BY station
                 ORDER BY listeners DESC, station ASC
                 LIMIT $2",
                &[&day, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| StationListeners {
                station: row.get("station"),
                listeners: row.get("listeners"),
            })
            .collect())
    }
}
