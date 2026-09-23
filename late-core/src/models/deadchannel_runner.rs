//! The runner row: a user's character in the deadchannel game (GAME.md,
//! Phase 2). This module owns every read and write of `deadchannel_runners`.
//!
//! The look is stored as JSON and handed back as a `serde_json::Value`: the
//! piece table that gives the codes meaning lives in the app (art in code,
//! ownership in the database), so the typed parse happens there, at the
//! boundary, and this module stays a storage layer.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// Cross-process refresh channel. Any insert or update on
/// `deadchannel_runners` fires it (migration 172 trigger); a listener
/// re-reads every look rather than trusting the payload, which only names
/// the user for logs.
pub const DEADCHANNEL_RUNNER_CHANGED_CHANNEL: &str = "deadchannel_runner_changed";

crate::model! {
    table = "deadchannel_runners";
    params = DeadchannelRunnerParams;
    struct DeadchannelRunner {
        @generated
        pub left_at: Option<DateTime<Utc>>;

        @data
        pub user_id: Uuid,
        pub look: serde_json::Value,
    }
}

/// What `ensure_for_user` found. The statements are the only witness of it,
/// so it is reported rather than inferred; the invited join counts a runner
/// created exactly once, and a return is its own beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOrigin {
    Created,
    /// The runner had left and is back: same row, same look, `left_at`
    /// cleared.
    Returned,
    Existing,
}

impl DeadchannelRunner {
    /// Create the runner for `user_id` wearing `look`, bring back the one
    /// that left, or return the one already standing. One statement per
    /// outcome, in that order: a conditional insert, so two devices joining
    /// at once (on any replicas) create one runner and both see the same
    /// look; then the conditional clear, which only writes when there is a
    /// leave to undo, so a duplicate join costs every replica nothing. The
    /// loser's `look` is discarded in both of the later cases: the face is
    /// the character's, and the character outlives the leave.
    pub async fn ensure_for_user(
        client: &Client,
        user_id: Uuid,
        look: &serde_json::Value,
    ) -> Result<(Self, RunnerOrigin)> {
        let inserted = client
            .query_opt(
                "INSERT INTO deadchannel_runners (user_id, look)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO NOTHING
                 RETURNING *",
                &[&user_id, look],
            )
            .await
            .context("inserting deadchannel runner")?;
        if let Some(row) = inserted {
            return Ok((Self::from(row), RunnerOrigin::Created));
        }
        let returned = client
            .query_opt(
                "UPDATE deadchannel_runners
                 SET left_at = NULL, updated = current_timestamp
                 WHERE user_id = $1 AND left_at IS NOT NULL
                 RETURNING *",
                &[&user_id],
            )
            .await
            .context("clearing deadchannel runner leave")?;
        if let Some(row) = returned {
            return Ok((Self::from(row), RunnerOrigin::Returned));
        }
        let row = client
            .query_one(
                "SELECT * FROM deadchannel_runners WHERE user_id = $1",
                &[&user_id],
            )
            .await
            .context("reading existing deadchannel runner")?;
        Ok((Self::from(row), RunnerOrigin::Existing))
    }

    /// `/leave #deadchannel`: close the door without burning the character.
    /// The row, its id, and its look stay; only the stamp lands, and the
    /// migration 172 trigger carries it to every replica, which is what
    /// shuts the undercity gate everywhere. Conditional on the stamp being
    /// absent, so leaving twice writes once and notifies once.
    ///
    /// Returns whether this call was the one that closed the door.
    pub async fn mark_left(client: &Client, user_id: Uuid) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_runners
                 SET left_at = current_timestamp, updated = current_timestamp
                 WHERE user_id = $1 AND left_at IS NULL
                 RETURNING id",
                &[&user_id],
            )
            .await
            .context("marking deadchannel runner left")?;
        Ok(row.is_some())
    }

    /// The row as it stands, whether or not the runner has left; read
    /// `left_at` to tell. Storage layer: who counts as a runner is the
    /// directory's question, and it asks `list_looks`.
    pub async fn find_by_user(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_runners WHERE user_id = $1",
                &[&user_id],
            )
            .await
            .context("finding deadchannel runner")?;
        Ok(row.map(Self::from))
    }

    /// Every standing runner's look, for the process-shared directory that
    /// paints portraits and gates the undercity. Runners are few by
    /// construction (the invitation gate), so the whole table is one read.
    /// A runner who left is absent here: the gate closes on every replica,
    /// and their old messages lose their portrait, which is the point of
    /// going dark.
    pub async fn list_looks(client: &Client) -> Result<Vec<(Uuid, serde_json::Value)>> {
        let rows = client
            .query(
                "SELECT user_id, look FROM deadchannel_runners WHERE left_at IS NULL",
                &[],
            )
            .await
            .context("listing deadchannel runner looks")?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("user_id"), row.get("look")))
            .collect())
    }
}

#[cfg(test)]
#[path = "deadchannel_runner_test.rs"]
mod deadchannel_runner_test;
