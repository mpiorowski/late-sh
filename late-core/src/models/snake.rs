use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "snake_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub score: i32,
        pub level: i32,
        pub lives: i32,
        pub is_game_over: bool,
    }
}

crate::user_scoped_model! {
    table = "snake_high_scores";
    user_field = user_id;
    params = HighScoreParams;
    struct HighScore {
        @data
        pub user_id: Uuid,
        pub score: i32,
    }
}

impl Game {
    pub async fn upsert(client: &Client, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO snake_games (user_id, score, level, lives, is_game_over)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (user_id) DO UPDATE SET
                    score = $2,
                    level = $3,
                    lives = $4,
                    is_game_over = $5,
                    updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.score,
                    &params.level,
                    &params.lives,
                    &params.is_game_over,
                ],
            )
            .await?;
        Ok(Self::from(row))
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
                "INSERT INTO snake_high_scores (user_id, score)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(snake_high_scores.score, $2), updated = current_timestamp
                 RETURNING *",
                &[&user_id, &new_score],
            )
            .await?;
        Ok(Self::from(row))
    }
}
