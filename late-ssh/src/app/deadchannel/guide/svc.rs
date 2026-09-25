//! The first descent's claim (root CONTEXT.md, multi-replica rule): one
//! conditional update on the runner row says whether this descent is the
//! first, so the guide opens by itself exactly once per runner, on
//! whichever device and replica got there first. Orchestration only: the
//! span, the log line per failure mode, and the reply.

use late_core::db::Db;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use tokio::sync::mpsc;
use tracing::{Instrument, info_span};
use uuid::Uuid;

/// What a descent came back with. Sent to the asking session only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuideOutcome {
    /// This was the runner's first descent: the guide opens.
    FirstDescent,
    /// Seen before, or no standing runner: the street stays as it is.
    SeenBefore,
}

#[derive(Clone)]
pub struct GuideService {
    db: Db,
}

impl GuideService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Claim the first descent for `user_id` and answer on `reply`. A
    /// failed claim answers nothing: the guide is a key away either way,
    /// and the next descent asks again.
    pub(crate) fn claim_first_descent_task(
        &self,
        user_id: Uuid,
        reply: mpsc::UnboundedSender<GuideOutcome>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.guide.claim_first_descent_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let outcome = match svc.claim_first_descent(user_id).await {
                    Ok(true) => {
                        tracing::info!("first descent: the guide opens");
                        GuideOutcome::FirstDescent
                    }
                    Ok(false) => GuideOutcome::SeenBefore,
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to claim the first descent");
                        return;
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    async fn claim_first_descent(&self, user_id: Uuid) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        DeadchannelRunner::mark_guide_seen(&client, user_id).await
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
