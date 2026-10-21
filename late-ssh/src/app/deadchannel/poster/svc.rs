//! The poster's reader and writer: PRINTS reads what hangs on the
//! standing runner's row when its panel opens, puts a poster up
//! (`DeadchannelRunner::store_poster`), and burns it
//! (`DeadchannelRunner::clear_poster`). No trigger carries a poster: a
//! profile reads the row when it opens, so the next open shows it.
//!
//! Orchestration only: the span, the metric, the log line per failure
//! mode, and the reply. `state.rs` is the editor and returns values; the
//! decode of the stored JSON happens here, at the boundary, and a row that
//! does not decode is logged and treated as a bare wall (the directory's
//! rule for looks), so saving over it mends it.

use anyhow::{Context, Result};
use dartboard_core::Canvas;
use late_core::db::Db;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use tokio::sync::mpsc;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::metrics::{self, PosterBeat};

/// What a session's read or write came back with. Sent to the asking
/// session only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PosterOutcome {
    /// What hangs on the wall, read for the panel; `None` is a bare wall.
    Hung(Option<Canvas>),
    /// The wall could not be read.
    Unread,
    /// The row holds this canvas now.
    Saved(Canvas),
    /// The wall is bare.
    Burned,
    /// No standing runner: the door is shut, or was never opened.
    NoRunner,
    Failed,
}

#[derive(Clone)]
pub struct PosterService {
    db: Db,
}

impl PosterService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Read the poster on the runner's wall and answer on `reply`.
    pub(crate) fn read_task(
        &self,
        user_id: Uuid,
        username: String,
        reply: mpsc::UnboundedSender<PosterOutcome>,
    ) {
        let svc = self.clone();
        let span =
            info_span!("deadchannel.poster.read_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let outcome = match svc.read(user_id).await {
                    Ok(Some(value)) => match serde_json::from_value::<Canvas>(value) {
                        Ok(canvas) => PosterOutcome::Hung(Some(canvas)),
                        Err(error) => {
                            tracing::error!(error = %error, "poster failed to decode; prints opens on a bare wall");
                            PosterOutcome::Hung(None)
                        }
                    },
                    Ok(None) => PosterOutcome::Hung(None),
                    Err(error) => {
                        metrics::record_deadchannel_poster(PosterBeat::Failed);
                        tracing::error!(error = ?error, "failed to read the runner poster");
                        PosterOutcome::Unread
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// Put `canvas` on the runner's row and answer on `reply`.
    pub(crate) fn save_task(
        &self,
        user_id: Uuid,
        username: String,
        canvas: Canvas,
        reply: mpsc::UnboundedSender<PosterOutcome>,
    ) {
        let svc = self.clone();
        let span =
            info_span!("deadchannel.poster.save_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let outcome = match svc.save(user_id, &canvas).await {
                    Ok(true) => {
                        metrics::record_deadchannel_poster(PosterBeat::Saved);
                        tracing::info!("runner poster saved");
                        PosterOutcome::Saved(canvas)
                    }
                    Ok(false) => {
                        metrics::record_deadchannel_poster(PosterBeat::NoRunner);
                        tracing::warn!("poster for a user with no standing runner");
                        PosterOutcome::NoRunner
                    }
                    Err(error) => {
                        metrics::record_deadchannel_poster(PosterBeat::Failed);
                        tracing::error!(error = ?error, "failed to store the runner poster");
                        PosterOutcome::Failed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// Take the poster off the runner's row and answer on `reply`.
    pub(crate) fn burn_task(
        &self,
        user_id: Uuid,
        username: String,
        reply: mpsc::UnboundedSender<PosterOutcome>,
    ) {
        let svc = self.clone();
        let span =
            info_span!("deadchannel.poster.burn_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let outcome = match svc.burn(user_id).await {
                    Ok(true) => {
                        metrics::record_deadchannel_poster(PosterBeat::Burned);
                        tracing::info!("runner poster burned");
                        PosterOutcome::Burned
                    }
                    Ok(false) => {
                        metrics::record_deadchannel_poster(PosterBeat::NoRunner);
                        tracing::warn!("poster burn for a user with no standing runner");
                        PosterOutcome::NoRunner
                    }
                    Err(error) => {
                        metrics::record_deadchannel_poster(PosterBeat::Failed);
                        tracing::error!(error = ?error, "failed to burn the runner poster");
                        PosterOutcome::Failed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    async fn read(&self, user_id: Uuid) -> Result<Option<serde_json::Value>> {
        let client = self.db.get().await?;
        DeadchannelRunner::poster(&client, user_id).await
    }

    async fn save(&self, user_id: Uuid, canvas: &Canvas) -> Result<bool> {
        let poster = serde_json::to_value(canvas).context("serializing the poster")?;
        let client = self.db.get().await?;
        DeadchannelRunner::store_poster(&client, user_id, &poster).await
    }

    async fn burn(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        DeadchannelRunner::clear_poster(&client, user_id).await
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
