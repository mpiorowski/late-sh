// Finished roguelike-door games, ingested from the door hosts' append-only
// xlog files over the stats SSH session (devdocs/PLAN-ROGUELIKE-BOARDS.md).
// One row per finished game; the (game, source_file, source_offset) unique
// key makes re-ingest after a cursor reset a no-op, which is the whole
// idempotency story. All parsing lives in late-ssh; this module owns only the
// table's data ops.

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use uuid::Uuid;

/// How a door run ended, the closed roster behind `door_runs.result`. The
/// wins boards count [`Self::WINS`]; everything else is history and depth/
/// score fodder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorRunResult {
    /// Died in the dungeon.
    Death,
    /// Quit the run (DCSS `ktyp=quitting`, NetHack `death=quit`).
    Quit,
    /// Left the dungeon alive without winning (DCSS `ktyp=leaving`, NetHack
    /// `death=escaped`).
    Leaving,
    /// Won the game (DCSS: escaped with the Orb, `ktyp=winning`; NetHack:
    /// `death=ascended`).
    Win,
    /// A win beyond winning (Brogue's Mastered, Phase 3). No DCSS mapping.
    Mastery,
}

impl DoorRunResult {
    /// The persisted `door_runs.result` value.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Death => "death",
            Self::Quit => "quit",
            Self::Leaving => "leaving",
            Self::Win => "win",
            Self::Mastery => "mastery",
        }
    }

    /// The results the per-game wins boards count.
    pub const WINS: &'static [Self] = &[Self::Win, Self::Mastery];
}

/// One parsed run, ready to land. `raw` carries the full parsed log line for
/// boards not invented yet.
#[derive(Clone, Debug)]
pub struct NewDoorRun {
    pub game: &'static str,
    pub user_id: Uuid,
    pub ended_at: DateTime<Utc>,
    pub result: DoorRunResult,
    pub score: Option<i64>,
    pub depth: Option<i32>,
    pub turns: Option<i64>,
    pub raw: serde_json::Value,
    pub source_file: String,
    pub source_offset: i64,
}

pub struct DoorRun;

impl DoorRun {
    /// Insert the run unless its source line already landed. Returns whether
    /// the row is fresh; a replayed line (cursor reset, reconnect overlap)
    /// returns false and changes nothing.
    pub async fn insert_ignore(client: &impl GenericClient, run: &NewDoorRun) -> Result<bool> {
        let inserted = client
            .execute(
                "INSERT INTO door_runs
                   (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (game, source_file, source_offset) DO NOTHING",
                &[
                    &run.game,
                    &run.user_id,
                    &run.ended_at,
                    &run.result.key(),
                    &run.score,
                    &run.depth,
                    &run.turns,
                    &run.raw,
                    &run.source_file,
                    &run.source_offset,
                ],
            )
            .await?;
        Ok(inserted > 0)
    }
}
