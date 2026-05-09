use anyhow::Result;
use late_core::db::Db;
use late_core::models::tetris::{Game, GameParams, HighScore};
use uuid::Uuid;

#[derive(Clone)]
pub struct TetrisService {
    db: Db,
}

impl TetrisService {
    pub fn new(db: Db) -> Self {
        Self { db }
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
                tracing::error!(error = ?e, "failed to save tetris game state");
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
                tracing::error!(error = ?e, "failed to submit tetris high score");
            }
        });
    }

    async fn submit_score(&self, user_id: Uuid, score: i32, final_score: bool) -> Result<()> {
        let client = self.db.get().await?;
        HighScore::update_score_if_higher(&client, user_id, score).await?;
        if final_score {
            client
                .execute(
                    "INSERT INTO game_score_events (user_id, game, score)
                     VALUES ($1, 'tetris', $2)",
                    &[&user_id, &score],
                )
                .await?;
        }
        Ok(())
    }
}
