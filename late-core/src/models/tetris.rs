use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "tetris_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub score: i32,
        pub lines: i32,
        pub level: i32,
        pub board: Value,
        pub current_kind: String,
        pub current_rotation: i32,
        pub current_row: i32,
        pub current_col: i32,
        pub next_kind: String,
        pub is_game_over: bool,
    }
}

crate::user_scoped_model! {
    table = "tetris_high_scores";
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
                "INSERT INTO tetris_games (user_id, score, lines, level, board, current_kind, current_rotation, current_row, current_col, next_kind, is_game_over)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT (user_id) DO UPDATE SET
                    score = $2,
                    lines = $3,
                    level = $4,
                    board = $5,
                    current_kind = $6,
                    current_rotation = $7,
                    current_row = $8,
                    current_col = $9,
                    next_kind = $10,
                    is_game_over = $11,
                    updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.score,
                    &params.lines,
                    &params.level,
                    &params.board,
                    &params.current_kind,
                    &params.current_rotation,
                    &params.current_row,
                    &params.current_col,
                    &params.next_kind,
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
                "INSERT INTO tetris_high_scores (user_id, score)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(tetris_high_scores.score, $2), updated = current_timestamp
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
                 VALUES ($1, 'tetris', $2)",
                &[&user_id, &score],
            )
            .await?;
        Ok(())
    }
}
