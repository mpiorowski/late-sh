// Which accounts have finished A Dark Room.
//
// Winning the ascent deletes the user's `darkroom_saves` row, so the save can
// never carry anything forward. This table is deliberately outside that blob:
// it is the only record that an account has ever finished the game, and
// finishing once is what puts the ravaged battleship on every later map.
//
// One fact, because one thing reads it. A Dark Room pays badges, not
// standings: which endings an account reached lives permanently in its
// `profile_awards` rows, so there is nothing here to count.

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

pub struct DarkroomVeteran;

impl DarkroomVeteran {
    /// Whether this account has ever got off the rock, either ending. The
    /// battleship is gated on exactly this.
    pub async fn has_escaped(client: &Client, user_id: Uuid) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT 1 FROM darkroom_veterans WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.is_some())
    }

    /// Record that this account has finished. Idempotent per user: a second
    /// escape is not a second row, and there is nothing to double-count.
    pub async fn record(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO darkroom_veterans (user_id)
                 VALUES ($1)
                 ON CONFLICT (user_id) DO NOTHING",
                &[&user_id],
            )
            .await?;
        Ok(())
    }
}
