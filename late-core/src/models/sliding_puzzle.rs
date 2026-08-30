use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::Difficulty;

crate::user_scoped_model! {
    table = "sliding_puzzle_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub mode: String,
        pub difficulty_key: String,
        pub puzzle_date: Option<NaiveDate>,
        pub puzzle_seed: i64,
        pub tiles: Vec<i32>,
        pub moves: i32,
    }
}

crate::user_scoped_model! {
    table = "sliding_puzzle_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub difficulty_key: String,
        pub puzzle_date: NaiveDate,
        pub moves: i32,
    }
}

#[derive(Debug)]
pub struct WinRecord {
    pub win: DailyWin,
    pub fresh: bool,
}

impl Game {
    pub async fn upsert(client: &Client, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO sliding_puzzle_games
                   (user_id, mode, difficulty_key, puzzle_date, puzzle_seed, tiles, moves)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (user_id, difficulty_key, mode) DO UPDATE SET
                   puzzle_date = $4,
                   puzzle_seed = $5,
                   tiles = $6,
                   moves = $7,
                   updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.mode,
                    &params.difficulty_key,
                    &params.puzzle_date,
                    &params.puzzle_seed,
                    &params.tiles,
                    &params.moves,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        moves: i32,
    ) -> Result<WinRecord> {
        let row = client
            .query_one(
                &format!(
                    "WITH win AS (
                         INSERT INTO sliding_puzzle_daily_wins
                           (user_id, difficulty_key, puzzle_date, moves)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT (user_id, difficulty_key, puzzle_date) DO UPDATE SET
                           moves = LEAST(sliding_puzzle_daily_wins.moves, $4),
                           updated = current_timestamp
                         RETURNING *, (xmax = 0) AS fresh_win
                     ),
                     total AS (
                         {bump}
                     )
                     SELECT * FROM win",
                    bump = super::leaderboard::bump_daily_win_total_sql(
                        super::leaderboard::DailyPuzzle::SlidingPuzzle
                    ),
                ),
                &[&user_id, &difficulty.key(), &puzzle_date, &moves],
            )
            .await?;
        let fresh = row.get("fresh_win");
        Ok(WinRecord {
            win: Self::from(row),
            fresh,
        })
    }

    pub async fn find(
        client: &Client,
        user_id: Uuid,
        difficulty_key: &str,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM sliding_puzzle_daily_wins
                 WHERE user_id = $1 AND difficulty_key = $2 AND puzzle_date = $3",
                &[&user_id, &difficulty_key, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}
