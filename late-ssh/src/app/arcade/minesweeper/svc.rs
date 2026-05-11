use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::app::arcade::chips::svc::ChipService;
use late_core::models::minesweeper::{DailyWin, Game, GameParams};
use late_core::models::profile::fetch_username;

#[derive(Clone)]
pub struct MinesweeperService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    chip_service: ChipService,
}

impl MinesweeperService {
    pub fn new(
        db: Db,
        activity_feed: broadcast::Sender<ActivityEvent>,
        chip_service: ChipService,
    ) -> Self {
        Self {
            db,
            activity_feed,
            chip_service,
        }
    }

    pub fn get_daily_seed(&self, difficulty_key: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let today = self.today();
        let mut hasher = std::hash::DefaultHasher::new();
        difficulty_key.hash(&mut hasher);
        today.format("%Y-%m-%d").to_string().hash(&mut hasher);
        "late-sh-minesweeper-salt".hash(&mut hasher);
        hasher.finish()
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub async fn load_games(&self, user_id: Uuid) -> Result<Vec<Game>> {
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.save_game(params).await {
                tracing::error!(error = ?e, "failed to save minesweeper game state");
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
            if let Err(e) = svc.record_win(user_id, difficulty_key.clone(), score).await {
                tracing::error!(error = ?e, "failed to record minesweeper daily win");
                return;
            }
            svc.chip_service
                .grant_daily_bonus_task(user_id, difficulty_key.clone());
            if let Ok(client) = svc.db.get().await {
                let username = fetch_username(&client, user_id).await;
                let _ = svc.activity_feed.send(ActivityEvent::game_won(
                    user_id,
                    username,
                    ActivityGame::Minesweeper,
                    Some(difficulty_key.clone()),
                    Some(score),
                ));
            }
        });
    }

    async fn record_win(&self, user_id: Uuid, difficulty_key: String, score: i32) -> Result<()> {
        let client = self.db.get().await?;
        let puzzle_date = self.today();
        DailyWin::record_win(&client, user_id, difficulty_key, puzzle_date, score).await?;
        Ok(())
    }
}
