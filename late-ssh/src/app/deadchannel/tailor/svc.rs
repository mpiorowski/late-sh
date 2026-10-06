//! The look's writer after the join: the tailor's mirror puts a look on
//! the standing runner's row (`DeadchannelRunner::store_look`), and the
//! row's change trigger carries it to every replica's look directory
//! (`runner/svc.rs`), which is how this session, every other session of
//! the runner, and everyone reading the wire see the new face.
//!
//! Orchestration only: the span, the metric, the log line per failure
//! mode, and the reply. `state.rs` is the editor and returns values.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use tokio::sync::mpsc;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::deadchannel::runner::state::Look;
use crate::metrics::{self, TailorBeat};

/// What a session's `wear` came back with. Sent to the asking session only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TailorOutcome {
    /// The row wears `Look` now.
    Worn(Look),
    /// No standing runner to dress: the door is shut or was never opened.
    NoRunner,
    Failed,
}

#[derive(Clone)]
pub struct TailorService {
    db: Db,
}

impl TailorService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Put `look` on the runner's row and answer on `reply`.
    pub(crate) fn wear_task(
        &self,
        user_id: Uuid,
        look: Look,
        reply: mpsc::UnboundedSender<TailorOutcome>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.tailor.wear_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let outcome = match svc.wear(user_id, look).await {
                    Ok(true) => {
                        metrics::record_deadchannel_tailor(TailorBeat::Worn);
                        tracing::info!(look = %look.to_json(), "runner look worn");
                        TailorOutcome::Worn(look)
                    }
                    Ok(false) => {
                        metrics::record_deadchannel_tailor(TailorBeat::NoRunner);
                        tracing::warn!("look for a user with no standing runner");
                        TailorOutcome::NoRunner
                    }
                    Err(error) => {
                        metrics::record_deadchannel_tailor(TailorBeat::Failed);
                        tracing::error!(error = ?error, "failed to store the runner look");
                        TailorOutcome::Failed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    async fn wear(&self, user_id: Uuid, look: Look) -> Result<bool> {
        let client = self.db.get().await?;
        DeadchannelRunner::store_look(&client, user_id, &look.to_json()).await
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
