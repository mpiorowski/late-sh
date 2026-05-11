use std::{sync::Arc, time::Duration};

use anyhow::Result;
use late_core::db::Db;
use late_core::models::leaderboard::fetch_leaderboard_data;
use tokio::sync::watch;

use late_core::models::leaderboard::LeaderboardData;

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
            // Initial load
            if let Err(e) = self.refresh().await {
                tracing::error!(error = ?e, "initial leaderboard refresh failed");
            }
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.tick().await; // first tick fires immediately, skip it
            loop {
                interval.tick().await;
                if let Err(e) = self.refresh().await {
                    tracing::warn!(error = ?e, "leaderboard refresh failed");
                }
            }
        })
    }
}
