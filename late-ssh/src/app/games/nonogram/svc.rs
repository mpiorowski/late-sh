use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::app::games::chips::svc::ChipService;
use late_core::models::nonogram::{DailyWin, Game, GameParams};
use late_core::models::profile::fetch_username;

#[derive(Clone)]
pub struct NonogramService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    chip_service: ChipService,
}

impl NonogramService {
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
            if let Err(error) = svc.save_game(params).await {
                tracing::error!(error = ?error, "failed to save nonogram game state");
            }
        });
    }

    async fn save_game(&self, params: GameParams) -> Result<()> {
        let client = self.db.get().await?;
        Game::upsert(&client, params).await?;
        Ok(())
    }

    pub async fn has_won_today(&self, user_id: Uuid, size_key: &str) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&client, user_id, size_key, self.today()).await
    }

    pub fn record_win_task(&self, user_id: Uuid, size_key: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.record_win(user_id, size_key.clone()).await {
                tracing::error!(error = ?error, "failed to record nonogram daily win");
                return;
            }
            svc.chip_service
                .grant_daily_bonus_task(user_id, size_key.clone());
            if let Ok(client) = svc.db.get().await {
                let username = fetch_username(&client, user_id).await;
                let _ = svc.activity_feed.send(ActivityEvent::game_won(
                    user_id,
                    username,
                    ActivityGame::Nonogram,
                    Some(size_key.clone()),
                    None,
                ));
            }
        });
    }

    async fn record_win(&self, user_id: Uuid, size_key: String) -> Result<()> {
        let client = self.db.get().await?;
        DailyWin::record_win(&client, user_id, size_key, self.today()).await?;
        Ok(())
    }
}
