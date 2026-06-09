use anyhow::Result;
use late_core::db::Db;
use late_core::models::tetris::{Game, GameParams, HighScore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::app::activity::publisher::ActivityPublisher;

#[derive(Clone)]
pub struct LaterisService {
    db: Db,
    activity: Option<ActivityPublisher>,
}

impl LaterisService {
    pub fn new(db: Db) -> Self {
        Self { db, activity: None }
    }

    pub fn with_activity_feed(mut self, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        self.activity = Some(ActivityPublisher::new(self.db.clone(), activity_feed));
        self
    }

    pub async fn load_game(&self, user_id: Uuid) -> Result<Option<Game>> {
        let client = self.db.get().await?;
        Game::find_by_user_id(&client, user_id).await
    }

    pub async fn load_high_score(&self, user_id: Uuid) -> Result<Option<HighScore>> {
        let client = self.db.get().await?;
        HighScore::find_by_user_id(&client, user_id).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.save_game(params).await {
                tracing::error!(error = ?e, "failed to save Lateris game state");
            }
        });
    }

    async fn save_game(&self, params: GameParams) -> Result<()> {
        let client = self.db.get().await?;
        Game::upsert(&client, params).await?;
        Ok(())
    }

    pub fn submit_score_task(&self, user_id: Uuid, score: i32, final_score: bool) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.submit_score(user_id, score, final_score).await {
                tracing::error!(error = ?e, "failed to submit Lateris high score");
            }
        });
    }

    async fn submit_score(&self, user_id: Uuid, score: i32, final_score: bool) -> Result<()> {
        let client = self.db.get().await?;
        HighScore::update_score_if_higher(&client, user_id, score).await?;
        if final_score {
            HighScore::record_score_event(&client, user_id, score).await?;
            if let Some(activity) = &self.activity {
                activity.game_scored_task(user_id, ActivityGame::Lateris, score, None);
            }
        }
        Ok(())
    }
}
