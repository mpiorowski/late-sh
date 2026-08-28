use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// How far a reader's `/summary` catch-up of one room has been carried.
///
/// The whole contract is in migration 165: this is what the reader was
/// *told*, not a guess about what they read, and it is stamped from a real
/// message rather than from `now()` so consecutive windows abut instead of
/// leaving a hole around the model call.
#[derive(Debug, Clone)]
pub struct ChatSummaryRead {
    pub user_id: Uuid,
    pub room_id: Uuid,
    pub summarized_through: DateTime<Utc>,
}

impl ChatSummaryRead {
    /// The end of this reader's last delivered summary of the room. `None`
    /// has one meaning only: they have never been handed a summary of it, so
    /// the caller opens with the first-catch-up window rather than reaching
    /// back forever.
    pub async fn summarized_through(
        client: &Client,
        user_id: Uuid,
        room_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "SELECT summarized_through
                 FROM chat_summary_reads
                 WHERE user_id = $1 AND room_id = $2",
                &[&user_id, &room_id],
            )
            .await?;
        Ok(row.map(|row| row.get("summarized_through")))
    }

    /// Carry the watermark to `through`, never backwards.
    ///
    /// `GREATEST` is load-bearing rather than defensive: an explicit
    /// `/summary 6h` run right after a catch-up delivers a window whose
    /// newest message is older than nothing, but two sessions racing, or an
    /// explicit window aimed at an already-summarized stretch, must not
    /// rewind the cursor and hand the reader the same bullets twice.
    pub async fn advance(
        client: &Client,
        user_id: Uuid,
        room_id: Uuid,
        through: DateTime<Utc>,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO chat_summary_reads (user_id, room_id, summarized_through, updated)
                 VALUES ($1, $2, $3, current_timestamp)
                 ON CONFLICT (user_id, room_id)
                 DO UPDATE SET
                   summarized_through = GREATEST(
                     chat_summary_reads.summarized_through,
                     EXCLUDED.summarized_through
                   ),
                   updated = current_timestamp",
                &[&user_id, &room_id, &through],
            )
            .await?;
        Ok(())
    }
}
