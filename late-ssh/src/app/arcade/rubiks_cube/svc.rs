use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::profile::fetch_username;
use late_core::models::rubiks_cube::DailyWin;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};

#[derive(Clone)]
pub struct RubiksCubeService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl RubiksCubeService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub async fn has_won_today(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&client, user_id, self.today()).await
    }

    pub fn record_win_task(&self, user_id: Uuid, puzzle_date: NaiveDate) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.record_win_and_publish(user_id, puzzle_date).await {
                tracing::error!(error = ?error, "failed to record Rubik's Cube daily win");
            }
        });
    }

    async fn record_win_and_publish(&self, user_id: Uuid, puzzle_date: NaiveDate) -> Result<()> {
        let client = self.db.get().await?;
        let Some(_) = DailyWin::record_win(&client, user_id, puzzle_date).await? else {
            return Ok(());
        };
        let username = fetch_username(&client, user_id).await;
        let _ = self.activity_feed.send(ActivityEvent::game_won_at(
            user_id,
            username,
            ActivityGame::RubiksCube,
            Some("daily".to_string()),
            None,
            ActivityEvent::occurred_on_utc_date(puzzle_date),
        ));
        Ok(())
    }
}
