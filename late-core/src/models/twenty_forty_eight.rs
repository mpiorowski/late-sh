use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "twenty_forty_eight_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub score: i32,
        pub grid: Value,
        pub is_game_over: bool,
    }
}

crate::user_scoped_model! {
    table = "twenty_forty_eight_high_scores";
    user_field = user_id;
    params = HighScoreParams;
    struct HighScore {
        @data
        pub user_id: Uuid,
        pub score: i32,
    }
}

impl Game {
    pub async fn upsert(
        client: &Client,
        user_id: Uuid,
        score: i32,
        grid: Value,
        is_game_over: bool,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO twenty_forty_eight_games (user_id, score, grid, is_game_over)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id) DO UPDATE SET score = $2, grid = $3, is_game_over = $4, updated = current_timestamp
                 RETURNING *",
                &[&user_id, &score, &grid, &is_game_over],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn clear(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "DELETE FROM twenty_forty_eight_games WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }
}

impl HighScore {
    pub async fn update_score_if_higher(
        client: &Client,
        user_id: Uuid,
        new_score: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO twenty_forty_eight_high_scores (user_id, score)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(twenty_forty_eight_high_scores.score, $2), updated = current_timestamp
                 RETURNING *",
                &[&user_id, &new_score],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn record_score_event(client: &Client, user_id: Uuid, score: i32) -> Result<()> {
        client
            .execute(
                "INSERT INTO game_score_events (user_id, game, score)
                 VALUES ($1, '2048', $2)",
                &[&user_id, &score],
            )
            .await?;
        Ok(())
    }
}
