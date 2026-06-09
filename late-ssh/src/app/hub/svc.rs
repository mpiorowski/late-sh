use std::{sync::Arc, time::Duration};

use anyhow::Result;
use late_core::db::Db;
use late_core::models::leaderboard::{LeaderboardData, fetch_leaderboard_data};
use late_core::models::profile_award::snapshot_previous_month_profile_awards;
use tokio::sync::watch;

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
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.tick().await;
            loop {
                interval.tick().await;
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
