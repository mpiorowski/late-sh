use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use late_core::models::profile::fetch_username;
use late_core::models::solitaire::{DailyWin, Game, GameParams};

#[derive(Clone)]
pub struct SolitaireService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl SolitaireService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    pub fn get_daily_seed(&self, difficulty_key: &str) -> u64 {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::hash::DefaultHasher::new();
        difficulty_key.hash(&mut hasher);
        self.today()
            .format("%Y-%m-%d")
            .to_string()
            .hash(&mut hasher);
        "late-sh-solitaire-salt".hash(&mut hasher);
        hasher.finish()
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub async fn load_games(&self, user_id: Uuid) -> Result<Vec<Game>> {
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub async fn has_won_today(&self, user_id: Uuid, difficulty_key: &str) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&client, user_id, difficulty_key, self.today()).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.save_game(params).await {
                tracing::error!(error = ?error, "failed to save solitaire game state");
            }
        });
    }

    async fn save_game(&self, params: GameParams) -> Result<()> {
        let client = self.db.get().await?;
        Game::upsert(&client, params).await?;
        Ok(())
    }

    pub fn record_win_task(&self, user_id: Uuid, difficulty_key: String, score: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            let puzzle_date = match svc.record_win(user_id, difficulty_key.clone(), score).await {
                Ok(puzzle_date) => puzzle_date,
                Err(error) => {
                    tracing::error!(error = ?error, "failed to record solitaire daily win");
                    return;
                }
            };
            let username = match svc.db.get().await {
                Ok(client) => fetch_username(&client, user_id).await,
                Err(error) => {
                    tracing::warn!(%user_id, ?error, "publishing solitaire win with fallback username");
                    "someone".to_string()
                }
            };
            let _ = svc.activity_feed.send(ActivityEvent::game_won_at(
                user_id,
                username,
                ActivityGame::Solitaire,
                Some(difficulty_key.clone()),
                Some(score),
                ActivityEvent::occurred_on_utc_date(puzzle_date),
            ));
        });
    }

    async fn record_win(
        &self,
        user_id: Uuid,
        difficulty_key: String,
        score: i32,
    ) -> Result<NaiveDate> {
        let client = self.db.get().await?;
        let puzzle_date = self.today();
        DailyWin::record_win(&client, user_id, difficulty_key, puzzle_date, score).await?;
        Ok(puzzle_date)
    }
}
