// Live mid-run milestones from the door hosts' milestone xlogs (DCSS's
// milestones file, NetHack's livelog). Same idempotency contract as
// door_runs: (game, source_file, source_offset) names the line. The Orb and
// Amulet badges grant from this stream so they land at pickup, not at game
// end.

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use uuid::Uuid;

/// The milestone kinds worth persisting, the closed roster behind
/// `door_milestones.kind`. Ingestion skips every other milestone type (the
/// cursor still advances past them); add a variant here when a new kind earns
/// a badge or a board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorMilestoneKind {
    /// Picked up the game's win-condition artifact (DCSS: the Orb of Zot).
    Orb,
    /// Collected a rune of Zot.
    Rune,
    /// Entered a dungeon branch (which one is in `raw`).
    BranchEnter,
    /// NetHack: acquired the Amulet of Yendor (from the livelog stream).
    Amulet,
}

impl DoorMilestoneKind {
    /// The persisted `door_milestones.kind` value.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Orb => "orb",
            Self::Rune => "rune",
            Self::BranchEnter => "br_enter",
            Self::Amulet => "amulet",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewDoorMilestone {
    pub game: &'static str,
    pub user_id: Uuid,
    pub kind: DoorMilestoneKind,
    pub occurred_at: DateTime<Utc>,
    pub raw: serde_json::Value,
    pub source_file: String,
    pub source_offset: i64,
}

pub struct DoorMilestone;

impl DoorMilestone {
    /// Insert the milestone unless its source line already landed. Returns
    /// whether the row is fresh.
    pub async fn insert_ignore(
        client: &impl GenericClient,
        milestone: &NewDoorMilestone,
    ) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO door_milestones
                   (game, user_id, kind, occurred_at, raw, source_file, source_offset)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (game, source_file, source_offset) DO NOTHING",
                &[
                    &milestone.game,
                    &milestone.user_id,
                    &milestone.kind.key(),
                    &milestone.occurred_at,
                    &milestone.raw,
                    &milestone.source_file,
                    &milestone.source_offset,
                ],
            )
            .await?;
        Ok(inserted > 0)
    }
}
