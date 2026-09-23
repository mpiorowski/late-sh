//! The runner look directory: every runner's look, served from Postgres to
//! every replica (root CONTEXT.md, multi-replica rule). Same shape as
//! `app/flags/svc.rs`: the process listener (`crate::pg_listener`) routes
//! `deadchannel_runner_changed` here and every look is re-read on any change;
//! sessions hold a `watch` receiver, copy it on the tick edge, and paint
//! portraits from the owned copy.
//!
//! Runners are few by construction (the invitation gate), so the whole
//! table is one read. A look that fails to parse is logged and skipped:
//! one bad row must not blank every portrait on the wire.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use late_core::db::Db;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::state::Look;
use crate::pg_listener::Signal;

/// What the directory serves: user id to look, shared by `Arc` so a
/// session's tick copy is a pointer bump.
pub type RunnerLooks = Arc<HashMap<Uuid, Look>>;

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
        let rows = DeadchannelRunner::list_looks(&client).await?;
        let mut looks = HashMap::with_capacity(rows.len());
        for (user_id, value) in rows {
            match Look::parse(&value) {
                Ok(look) => {
                    looks.insert(user_id, look);
                }
                Err(error) => {
                    tracing::error!(error = %error, user_id = %user_id, "runner look failed to parse; portrait skipped");
                }
            }
        }
        self.looks_tx.send_replace(Arc::new(looks));
        Ok(())
    }

    /// Keep every replica's looks in step with `deadchannel_runner_changed`
    /// (subscribed in `main.rs`). A resync and a notify are the same
    /// re-read, and a burst of changes collapses into one.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while signals.recv().await.is_some() {
                while signals.try_recv().is_ok() {}
                // A failed re-read is this replica lagging until the next
                // change or reconnect.
                if let Err(error) = service.refresh().await {
                    tracing::warn!(error = ?error, "failed to refresh runner looks");
                }
            }
        })
    }
}

/// A receiver already holding `looks`, for test apps and headless paths.
/// The sender is dropped on purpose; a `watch` receiver keeps serving the
/// last value.
pub fn fixed_looks_rx(looks: HashMap<Uuid, Look>) -> watch::Receiver<RunnerLooks> {
    let (_tx, rx) = watch::channel(Arc::new(looks));
    rx
}
