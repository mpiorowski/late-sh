//! Process-wide switches as rows (`app_flags`, migration 171): one row per
//! switch, keyed by the name this closed enum maps to, so a flip on one
//! replica is a flip on every replica and survives a restart (root
//! CONTEXT.md, multi-replica rule). A missing row is an error, never a
//! default: every switch the code knows is seeded by the migration that
//! adds its variant, and nobody ships an off-state by accident.
//!
//! Flipping one is a statement, not a key: `UPDATE app_flags SET enabled =
//! true WHERE key = 'herald_enabled'` through `scripts/connect_db.sh`.

use anyhow::{Result, bail};
use tokio_postgres::Client;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppFlag {
    /// The herald posts its daily picture (late-ssh `app/herald`). Seeded
    /// off by migration 252.
    HeraldEnabled,
}

impl AppFlag {
    pub fn key(self) -> &'static str {
        match self {
            Self::HeraldEnabled => "herald_enabled",
        }
    }

    /// The switch as it stands. An error for a row the seed never wrote.
    pub async fn is_enabled(self, client: &Client) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT enabled FROM app_flags WHERE key = $1",
                &[&self.key()],
            )
            .await?;
        match row {
            Some(row) => Ok(row.get("enabled")),
            None => bail!("app flag {} has no row", self.key()),
        }
    }

    /// Flip the switch. For tests and the operator's tooling; the app
    /// itself only reads.
    pub async fn set(self, client: &Client, enabled: bool) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE app_flags SET enabled = $2, updated = current_timestamp WHERE key = $1",
                &[&self.key(), &enabled],
            )
            .await?;
        if updated != 1 {
            bail!("app flag {} has no row", self.key());
        }
        Ok(())
    }
}
