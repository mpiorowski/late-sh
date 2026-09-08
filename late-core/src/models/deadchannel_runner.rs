//! The runner row: a user's character in the deadchannel game (GAME.md,
//! Phase 2). This module owns every read and write of `deadchannel_runners`.
//!
//! The look is stored as JSON and handed back as a `serde_json::Value`: the
//! piece table that gives the codes meaning lives in the app (art in code,
//! ownership in the database), so the typed parse happens there, at the
//! boundary, and this module stays a storage layer.

use anyhow::{Context, Result};
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
        @data
        pub user_id: Uuid,
        pub look: serde_json::Value,
    }
}

/// Whether `ensure_for_user` wrote the row or found one already there.
/// The insert is the only witness of that, so it is reported rather than
/// inferred; the invited join counts a runner created exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOrigin {
    Created,
    Existing,
}

impl DeadchannelRunner {
    /// Create the runner for `user_id` wearing `look`, or return the one
    /// that already exists. A conditional insert, so two devices joining at
    /// once (on any replicas) create one runner and both see the same look;
    /// the loser's `look` is discarded and comes back as `Existing`.
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
        match inserted {
            Some(row) => Ok((Self::from(row), RunnerOrigin::Created)),
            None => {
                let row = client
                    .query_one(
                        "SELECT * FROM deadchannel_runners WHERE user_id = $1",
                        &[&user_id],
                    )
                    .await
                    .context("reading existing deadchannel runner")?;
                Ok((Self::from(row), RunnerOrigin::Existing))
            }
        }
    }

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

    /// Every runner's look, for the process-shared directory that paints
    /// portraits. Runners are few by construction (the invitation gate), so
    /// the whole table is one read.
    pub async fn list_looks(client: &Client) -> Result<Vec<(Uuid, serde_json::Value)>> {
        let rows = client
            .query("SELECT user_id, look FROM deadchannel_runners", &[])
            .await
            .context("listing deadchannel runner looks")?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("user_id"), row.get("look")))
            .collect())
    }
}

pub async fn listen_for_deadchannel_runner_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {DEADCHANNEL_RUNNER_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "deadchannel_runner_test.rs"]
mod deadchannel_runner_test;
