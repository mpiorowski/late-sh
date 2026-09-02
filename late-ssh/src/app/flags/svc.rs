//! Process-wide switches, served from Postgres to every replica (root
//! CONTEXT.md, multi-replica rule). One long-lived connection LISTENs on
//! `app_flag_changed` and re-reads the whole `app_flags` table on any
//! change; sessions hold a `watch` receiver and read it on the tick path,
//! so a flip costs the flipping session one write and everyone else
//! nothing.
//!
//! The watch carries `None` until the first successful load. Readers treat
//! `None` as "the switch is off" on purpose: a switch nobody could read is
//! not a switch anybody may assume is on.

use std::time::Duration;

use anyhow::Result;
use late_core::db::{Db, DbConfig};
use late_core::models::app_flag::{
    APP_FLAG_CHANGED_CHANNEL, AppFlag, AppFlags, listen_for_app_flag_changes,
};
use tokio::sync::{oneshot, watch};
use tracing::{Instrument, info_span};

#[derive(Clone)]
pub struct AppFlagService {
    db: Db,
    flags_tx: watch::Sender<Option<AppFlags>>,
}

impl AppFlagService {
    pub fn new(db: Db) -> Self {
        let (flags_tx, _) = watch::channel(None);
        Self { db, flags_tx }
    }

    /// The process-shared switches. `None` until the listener's first load.
    pub fn subscribe(&self) -> watch::Receiver<Option<AppFlags>> {
        self.flags_tx.subscribe()
    }

    /// Re-read the table and publish it. The startup seed and the notify
    /// handler are the same call, so a reconnecting listener cannot be left
    /// holding stale switches.
    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let flags = AppFlags::load(&client).await?;
        self.flags_tx.send_replace(Some(flags));
        Ok(())
    }

    /// Keep every replica's switches in step. Same shape as
    /// `CrownService::start_listener_task`: a dropped connection reconnects
    /// after five seconds and re-seeds.
    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = service.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "app flag postgres listener stopped");
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
        let listen = listen_for_app_flag_changes(&client);
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

        // Seeded after the LISTEN is live, so a flip committed between the
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
        if notification.channel() != APP_FLAG_CHANGED_CHANNEL {
            return;
        }
        // A failed re-read is this replica lagging until the next flip, not
        // a reason to drop the LISTEN connection.
        if let Err(error) = self.refresh().await {
            tracing::warn!(error = ?error, key = notification.payload(), "failed to refresh app flags");
        }
    }

    /// Flip one switch. The answer (the row written, or the error) comes
    /// back on the returned channel so the caller can tell the admin the
    /// truth: a kill switch that reports "off" before the row says so is
    /// the one banner that must not lie. The notify brings the new value
    /// back to this replica like any other, so the caller's own view
    /// updates on the tick after the round trip. Errors are the
    /// receiver's to log, once; a receiver that went away (the session
    /// ended) is the only case logged here.
    pub fn set_task(&self, flag: AppFlag, enabled: bool) -> oneshot::Receiver<Result<()>> {
        let (tx, rx) = oneshot::channel();
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    AppFlags::set(&client, flag, enabled).await
                }
                .await;
                if let Err(Err(error)) = tx.send(result) {
                    tracing::error!(error = ?error, key = flag.key(), enabled, "failed to set app flag; nobody left to tell");
                }
            }
            .instrument(info_span!("app_flags.set_task", key = flag.key(), enabled)),
        );
        rx
    }
}
