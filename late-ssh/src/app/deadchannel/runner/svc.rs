//! The runner look directory: every runner's look, served from Postgres to
//! every replica (root CONTEXT.md, multi-replica rule). Same shape as
//! `app/flags/svc.rs`: one long-lived connection LISTENs on
//! `deadchannel_runner_changed` and re-reads every look on any change;
//! sessions hold a `watch` receiver, copy it on the tick edge, and paint
//! portraits from the owned copy.
//!
//! Runners are few by construction (the invitation gate), so the whole
//! table is one read. A look that fails to parse is logged and skipped:
//! one bad row must not blank every portrait on the wire.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use late_core::db::{Db, DbConfig};
use late_core::models::deadchannel_runner::{
    DEADCHANNEL_RUNNER_CHANGED_CHANNEL, DeadchannelRunner, listen_for_deadchannel_runner_changes,
};
use tokio::sync::watch;
use uuid::Uuid;

use super::state::Look;

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

    /// Keep every replica's looks in step. A dropped connection reconnects
    /// after five seconds and re-seeds.
    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = service.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "runner look postgres listener stopped");
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }

    async fn listen_once(&self, db_config: &DbConfig) -> Result<()> {
        let mut config = tokio_postgres::Config::new();
        config.host(&db_config.host);
        config.port(db_config.port);
        config.user(&db_config.user);
        config.password(&db_config.password);
        config.dbname(&db_config.dbname);

        let (client, mut connection) = config.connect(tokio_postgres::NoTls).await?;
        let listen = listen_for_deadchannel_runner_changes(&client);
        tokio::pin!(listen);
        loop {
            tokio::select! {
                result = &mut listen => {
                    result?;
                    break;
                }
                message = std::future::poll_fn(|cx| connection.poll_message(cx)) => {
                    let Some(message) = message else {
                        return Ok(());
                    };
                    self.handle_notification(message?).await;
                }
            }
        }

        // Seeded after the LISTEN is live, so a join committed between the
        // two is caught by this read rather than dropped.
        self.refresh().await?;

        loop {
            let Some(message) = std::future::poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };
            self.handle_notification(message?).await;
        }
    }

    async fn handle_notification(&self, message: tokio_postgres::AsyncMessage) {
        let tokio_postgres::AsyncMessage::Notification(notification) = message else {
            return;
        };
        if notification.channel() != DEADCHANNEL_RUNNER_CHANGED_CHANNEL {
            return;
        }
        // A failed re-read is this replica lagging until the next change,
        // not a reason to drop the LISTEN connection.
        if let Err(error) = self.refresh().await {
            tracing::warn!(error = ?error, user_id = notification.payload(), "failed to refresh runner looks");
        }
    }
}

/// A receiver already holding `looks`, for test apps and headless paths.
/// The sender is dropped on purpose; a `watch` receiver keeps serving the
/// last value.
pub fn fixed_looks_rx(looks: HashMap<Uuid, Look>) -> watch::Receiver<RunnerLooks> {
    let (_tx, rx) = watch::channel(Arc::new(looks));
    rx
}
