use std::{sync::Arc, time::Duration};

use anyhow::Result;
use late_core::db::Db;
use late_core::models::leaderboard::{LeaderboardData, fetch_leaderboard_data};
use late_core::models::profile_award::snapshot_previous_month_profile_awards;
use tokio::sync::watch;

/// How often the leaderboard is rebuilt from the DB while at least one session
/// is watching it.
///
/// `fetch_leaderboard_data` is eleven aggregate queries costing about 100 ms of
/// database time per pass, and it is a timer, not a reaction to anything a user
/// did. At the old 30 s cadence it was 13% of all database execution time in
/// prod (2026-07-26 `pg_stat_statements` ranking, SCALE.md). The data is daily
/// and monthly standings, so minutes of staleness are invisible. The one
/// latency-sensitive consumer, the per-session chip balance read in
/// `app/tick.rs`, does not wait for this loop: chip mutations notify
/// `chip_user_changed` and `ShopService` pushes the new balance per user.
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct LeaderboardService {
    db: Db,
    data_tx: Arc<watch::Sender<Arc<LeaderboardData>>>,
}

impl LeaderboardService {
    pub fn new(db: Db) -> Self {
        let (tx, _) = watch::channel(Arc::new(LeaderboardData::default()));
        Self {
            db,
            data_tx: Arc::new(tx),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<LeaderboardData>> {
        self.data_tx.subscribe()
    }

    /// Whether any session is currently watching the leaderboard. Every SSH
    /// session subscribes at bootstrap, so this is "is anyone connected". A
    /// refresh with no subscribers is eleven aggregate queries published to
    /// nobody, so the loop skips it.
    fn has_subscribers(&self) -> bool {
        self.data_tx.receiver_count() > 0
    }

    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let data = fetch_leaderboard_data(&client).await?;
        let _ = self.data_tx.send(Arc::new(data));
        Ok(())
    }

    pub fn start_refresh_loop(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.refresh().await {
                tracing::error!(error = ?e, "initial leaderboard refresh failed");
            }
            let mut interval = tokio::time::interval(REFRESH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                interval.tick().await;
                if !self.has_subscribers() {
                    tracing::debug!("no leaderboard subscribers, skipping refresh");
                    continue;
                }
                if let Err(e) = self.refresh().await {
                    tracing::warn!(error = ?e, "leaderboard refresh failed");
                }
            }
        })
    }

    pub fn start_profile_award_snapshot_loop(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.snapshot_profile_awards().await {
                tracing::error!(error = ?e, "initial profile award snapshot failed");
            }

            let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = self.snapshot_profile_awards().await {
                    tracing::warn!(error = ?e, "profile award snapshot failed");
                }
            }
        })
    }

    async fn snapshot_profile_awards(&self) -> Result<()> {
        let client = self.db.get().await?;
        let changed = snapshot_previous_month_profile_awards(&client).await?;
        tracing::debug!(changed, "profile award snapshot refreshed");
        Ok(())
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
