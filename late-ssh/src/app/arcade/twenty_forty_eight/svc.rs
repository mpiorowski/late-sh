use anyhow::Result;
use late_core::db::Db;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use late_core::models::twenty_forty_eight::{Game, HighScore};

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::app::activity::publisher::ActivityPublisher;

#[derive(Clone)]
pub struct TwentyFortyEightService {
    db: Db,
    activity: Option<ActivityPublisher>,
}

impl TwentyFortyEightService {
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

    /// Fire-and-forget task to save the current game state
    pub fn save_game_task(&self, user_id: Uuid, score: i32, grid: Value, is_game_over: bool) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.save_game(user_id, score, grid, is_game_over).await {
                tracing::error!(error = ?e, "failed to save 2048 game state");
            }
        });
    }

    async fn save_game(
        &self,
        user_id: Uuid,
        score: i32,
        grid: Value,
        is_game_over: bool,
    ) -> Result<()> {
        let client = self.db.get().await?;
        Game::upsert(&client, user_id, score, grid, is_game_over).await?;
        Ok(())
    }

    /// Fire-and-forget task to submit a new score (only updates if it's a high score)
    pub fn submit_score_task(&self, user_id: Uuid, score: i32, final_score: bool) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.submit_score(user_id, score, final_score).await {
                tracing::error!(error = ?e, "failed to submit 2048 high score");
            }
        });
    }

    async fn submit_score(&self, user_id: Uuid, score: i32, final_score: bool) -> Result<()> {
        let client = self.db.get().await?;
        HighScore::update_score_if_higher(&client, user_id, score).await?;
        if final_score {
            HighScore::record_score_event(&client, user_id, score).await?;
            if let Some(activity) = &self.activity {
                activity.game_scored_task(user_id, ActivityGame::TwentyFortyEight, score, None);
            }
        }
        Ok(())
    }

    /// Fire-and-forget task to clear the saved game when restarting
    pub fn clear_game_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let client = match svc.db.get().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = ?e, "failed to get db client to clear 2048 game");
                    return;
                }
            };
            if let Err(e) = Game::clear(&client, user_id).await {
                tracing::error!(error = ?e, "failed to clear 2048 game state");
            }
        });
    }
}
