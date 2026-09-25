//! The runner directory: every standing runner's look and level, served
//! from Postgres to every replica (root CONTEXT.md, multi-replica rule).
//! Same shape as `app/flags/svc.rs`: the process listener
//! (`crate::pg_listener`) routes `deadchannel_runner_changed` here and the
//! whole directory is re-read on any change (look, standing, or level, per
//! the migration 202 trigger); sessions hold a `watch` receiver, copy it on
//! the tick edge, and paint portraits and level badges from the owned copy.
//!
//! Runners are few by construction (the invitation gate), so the whole
//! table is one read. A look that fails to parse is logged and skipped:
//! one bad row must not blank every portrait on the wire.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use late_core::db::Db;
use late_core::models::deadchannel_runner::{DeadchannelRunner, StandingRunner};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::state::Look;
use crate::pg_listener::{Channel, Refresh, Signal, read_until_ok};

/// One standing runner as the wire sees them: the face and the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerEntry {
    pub look: Look,
    pub level: i32,
}

/// What the directory serves: user id to entry, shared by `Arc` so a
/// session's tick copy is a pointer bump. Membership is standing: a
/// runner who left is absent.
pub type RunnerLooks = Arc<HashMap<Uuid, RunnerEntry>>;

#[derive(Clone)]
pub struct RunnerLookService {
    db: Db,
    looks_tx: watch::Sender<RunnerLooks>,
}

impl RunnerLookService {
    pub fn new(db: Db) -> Self {
        let (looks_tx, _) = watch::channel(Arc::new(HashMap::new()));
        Self { db, looks_tx }
    }

    /// The process-shared looks. Empty until the listener's first load.
    pub fn subscribe(&self) -> watch::Receiver<RunnerLooks> {
        self.looks_tx.subscribe()
    }

    /// Re-read every look and publish them. The startup seed and the
    /// notify handler are the same call, so a reconnecting listener cannot
    /// be left holding stale looks.
    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let rows = DeadchannelRunner::list_standing(&client).await?;
        let mut looks = HashMap::with_capacity(rows.len());
        for StandingRunner {
            user_id,
            look,
            level,
        } in rows
        {
            match Look::parse(&look) {
                Ok(look) => {
                    looks.insert(user_id, RunnerEntry { look, level });
                }
                Err(error) => {
                    tracing::error!(error = %error, user_id = %user_id, "runner look failed to parse; portrait skipped");
                }
            }
        }
        self.looks_tx.send_replace(Arc::new(looks));
        Ok(())
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::DeadchannelRunnerChanged];

    /// Keep every replica's looks in step with `deadchannel_runner_changed`.
    /// A resync and a notify are the same re-read, a burst of changes
    /// collapses into one, and a failed read retries until it lands: the
    /// resync read is what seeds this replica, so it may not be dropped.
    /// The payload (the user id, per the migration 172 trigger) is logged
    /// and never trusted: the read is the truth.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                log_signal(&signal);
                while let Ok(signal) = signals.try_recv() {
                    log_signal(&signal);
                }
                read_until_ok(Refresh::RunnerLooks, || service.refresh()).await;
            }
        })
    }
}

fn log_signal(signal: &Signal) {
    match signal {
        Signal::Resync => tracing::debug!("runner looks resync"),
        Signal::Notify { channel, payload } => {
            tracing::debug!(channel = ?channel, user_id = %payload, "runner changed")
        }
    }
}

/// A receiver already holding `looks`, for test apps and headless paths.
/// The sender is dropped on purpose; a `watch` receiver keeps serving the
/// last value.
pub fn fixed_looks_rx(looks: HashMap<Uuid, RunnerEntry>) -> watch::Receiver<RunnerLooks> {
    let (_tx, rx) = watch::channel(Arc::new(looks));
    rx
}
