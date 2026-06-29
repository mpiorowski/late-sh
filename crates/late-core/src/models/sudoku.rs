use anyhow::Result;
use chrono::NaiveDate;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "sudoku_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub mode: String,
        pub difficulty_key: String,
        pub puzzle_date: Option<NaiveDate>,
        pub puzzle_seed: i64,
        pub grid: Value,
        pub fixed_mask: Value,
        pub is_game_over: bool,
        pub score: i32,
    }
}

crate::user_scoped_model! {
    table = "sudoku_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub difficulty_key: String,
        pub puzzle_date: NaiveDate,
        pub score: i32,
    }
}

impl Game {
    pub async fn upsert(client: &Client, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO sudoku_games (user_id, mode, difficulty_key, puzzle_date, puzzle_seed, grid, fixed_mask, is_game_over, score)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (user_id, difficulty_key, mode) DO UPDATE SET puzzle_date = $4, puzzle_seed = $5, grid = $6, fixed_mask = $7, is_game_over = $8, score = $9, updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.mode,
                    &params.difficulty_key,
                    &params.puzzle_date,
                    &params.puzzle_seed,
                    &params.grid,
                    &params.fixed_mask,
                    &params.is_game_over,
                    &params.score,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn clear(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute("DELETE FROM sudoku_games WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(())
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        difficulty_key: String,
        puzzle_date: NaiveDate,
        score: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO sudoku_daily_wins (user_id, difficulty_key, puzzle_date, score)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, difficulty_key, puzzle_date) DO UPDATE SET score = GREATEST(sudoku_daily_wins.score, $4), updated = current_timestamp
                 RETURNING *",
                &[&user_id, &difficulty_key, &puzzle_date, &score],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn has_won_today(
        client: &Client,
        user_id: Uuid,
        difficulty_key: &str,
        puzzle_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id FROM sudoku_daily_wins WHERE user_id = $1 AND difficulty_key = $2 AND puzzle_date = $3",
                &[&user_id, &difficulty_key, &puzzle_date],
            )
            .await?;
        Ok(row.is_some())
    }
}
