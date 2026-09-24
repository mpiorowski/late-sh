//! Process-wide switches, served from Postgres to every replica (root
//! CONTEXT.md, multi-replica rule). The process listener
//! (`crate::pg_listener`) routes `app_flag_changed` here and this service
//! re-reads the whole `app_flags` table on any change; sessions hold a `watch` receiver and read it on the tick path,
//! so a flip costs the flipping session one write and everyone else
//! nothing.
//!
//! The watch carries `None` until the first successful load. Readers treat
//! `None` as "the switch is off" on purpose: a switch nobody could read is
//! not a switch anybody may assume is on.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::app_flag::{AppFlag, AppFlags};
use tokio::sync::{mpsc, oneshot, watch};

use crate::pg_listener::{Channel, Signal, read_until_ok};
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

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::AppFlagChanged];

    /// Keep this replica's switches in step with `app_flag_changed`. A
    /// resync and a notify are the same re-read, a burst of flips collapses
    /// into one, and a failed read retries until it lands: the resync read
    /// is what seeds this replica, so it may not be dropped.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while signals.recv().await.is_some() {
                while signals.try_recv().is_ok() {}
                read_until_ok("app flags", || service.refresh()).await;
            }
        })
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
