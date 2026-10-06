//! Lines carved into the bar out back (migration 190): one per stool,
//! overwritten by whoever sits there next. All data ops for the table live
//! here; the screen that shows them is `late-ssh/src/app/clubhouse/nightcap`.

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use tokio_postgres::{Client, Row};
use uuid::Uuid;

/// Stools at the bar; the table's check constraint mirrors it.
pub const STOOL_COUNT: i16 = 6;
/// A carving is a line, not a paragraph.
pub const CARVING_MAX_CHARS: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carving {
    pub stool: i16,
    /// Who carved it; `None` once that account is gone.
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub body: String,
    pub updated: DateTime<Utc>,
}

impl From<Row> for Carving {
    fn from(row: Row) -> Self {
        Self {
            stool: row.get("stool"),
            user_id: row.get("user_id"),
            username: row.get("username"),
            body: row.get("body"),
            updated: row.get("updated"),
        }
    }
}

impl Carving {
    /// Every carved stool, lowest stool first.
    pub async fn list(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT c.stool, c.user_id, u.username, c.body, c.updated
                 FROM nightcap_carvings c
                 LEFT JOIN users u ON u.id = c.user_id
                 ORDER BY c.stool ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Carve a line into a stool, over whatever was there. The body is
    /// trimmed and kept to one line of at most [`CARVING_MAX_CHARS`]; an
    /// empty line is refused rather than carved as nothing.
    pub async fn carve(client: &Client, stool: i16, user_id: Uuid, body: &str) -> Result<Self> {
        if !(0..STOOL_COUNT).contains(&stool) {
            bail!("no stool {stool} at the bar");
        }
        let body: String = body
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(CARVING_MAX_CHARS)
            .collect();
        if body.is_empty() {
            bail!("nothing to carve");
        }
        let row = client
            .query_one(
                "WITH carved AS (
                     INSERT INTO nightcap_carvings (stool, user_id, body)
                     VALUES ($1, $2, $3)
                     ON CONFLICT (stool) DO UPDATE
                        SET user_id = EXCLUDED.user_id,
                            body = EXCLUDED.body,
                            updated = current_timestamp
                     RETURNING stool, user_id, body, updated
                 )
                 SELECT c.stool, c.user_id, u.username, c.body, c.updated
                 FROM carved c
                 LEFT JOIN users u ON u.id = c.user_id",
                &[&stool, &user_id, &body],
            )
            .await?;
        Ok(Self::from(row))
    }
}

#[cfg(test)]
#[path = "nightcap_carving_test.rs"]
mod nightcap_carving_test;
