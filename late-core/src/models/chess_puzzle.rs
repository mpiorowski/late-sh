use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::chips::Difficulty;

crate::model! {
    table = "chess_puzzle_daily_puzzles";
    params = DailyPickParams;
    struct DailyPick {
        @data
        pub puzzle_date: NaiveDate,
        pub difficulty_key: String,
        pub puzzle_id: String,
    }
}

crate::user_scoped_model! {
    table = "chess_puzzle_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub mode: String,
        pub difficulty_key: String,
        pub puzzle_date: Option<NaiveDate>,
        pub puzzle_id: String,
        pub ply: i32,
        pub mistake_plies: Vec<i32>,
        pub mate_move: Option<String>,
    }
}

crate::user_scoped_model! {
    table = "chess_puzzle_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub difficulty_key: String,
        pub puzzle_date: NaiveDate,
        pub mistakes: i32,
    }
}

#[derive(Debug)]
pub struct WinRecord {
    pub win: DailyWin,
    pub fresh: bool,
}

impl DailyPick {
    /// The day's claimed puzzles, one row per tier claimed so far.
    pub async fn find_by_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM chess_puzzle_daily_puzzles WHERE puzzle_date = $1",
                &[&puzzle_date],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Take the transaction-scoped advisory lock guarding the day's claims,
    /// so concurrent `ensure_daily_puzzles` callers serialize on the
    /// check-then-insert instead of racing. Released when the transaction
    /// ends, so the caller must hold an open transaction.
    pub async fn lock_daily_creation(client: &impl GenericClient) -> Result<()> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('chess_puzzle_daily_puzzle', 0))",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Every puzzle ever claimed on one tier, with the last day it was
    /// claimed for.
    pub async fn last_used(
        client: &impl GenericClient,
        difficulty_key: &str,
    ) -> Result<HashMap<String, NaiveDate>> {
        let rows = client
            .query(
                "SELECT puzzle_id, MAX(puzzle_date) AS last_used
                 FROM chess_puzzle_daily_puzzles
                 WHERE difficulty_key = $1
                 GROUP BY puzzle_id",
                &[&difficulty_key],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("puzzle_id"), row.get("last_used")))
            .collect())
    }

    pub async fn insert_for_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
        difficulty_key: &str,
        puzzle_id: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO chess_puzzle_daily_puzzles (puzzle_date, difficulty_key, puzzle_id)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (puzzle_date, difficulty_key)
                 DO UPDATE SET puzzle_id = chess_puzzle_daily_puzzles.puzzle_id
                 RETURNING *",
                &[&puzzle_date, &difficulty_key, &puzzle_id],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl Game {
    /// The stored row of one slot, locked until the caller's transaction
    /// ends, so a save or a win decided on what it holds cannot interleave
    /// with another session's.
    pub async fn lock_slot(
        client: &impl GenericClient,
        user_id: Uuid,
        difficulty_key: &str,
        mode: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM chess_puzzle_games
                 WHERE user_id = $1 AND difficulty_key = $2 AND mode = $3
                 FOR UPDATE",
                &[&user_id, &difficulty_key, &mode],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert(client: &impl GenericClient, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO chess_puzzle_games
                   (user_id, mode, difficulty_key, puzzle_date, puzzle_id, ply, mistake_plies,
                    mate_move)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (user_id, difficulty_key, mode) DO UPDATE SET
                   puzzle_date = $4,
                   puzzle_id = $5,
                   ply = $6,
                   mistake_plies = $7,
                   mate_move = $8,
                   updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.mode,
                    &params.difficulty_key,
                    &params.puzzle_date,
                    &params.puzzle_id,
                    &params.ply,
                    &params.mistake_plies,
                    &params.mate_move,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &impl GenericClient,
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        mistakes: i32,
    ) -> Result<WinRecord> {
        let row = client
            .query_one(
                &format!(
                    "WITH win AS (
                         INSERT INTO chess_puzzle_daily_wins
                           (user_id, difficulty_key, puzzle_date, mistakes)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT (user_id, difficulty_key, puzzle_date) DO UPDATE SET
                           mistakes = LEAST(chess_puzzle_daily_wins.mistakes, $4),
                           updated = current_timestamp
                         RETURNING *, (xmax = 0) AS fresh_win
                     ),
                     total AS (
                         {bump}
                     )
                     SELECT * FROM win",
                    bump = super::leaderboard::bump_daily_win_total_sql(
                        super::leaderboard::DailyPuzzle::ChessPuzzle
                    ),
                ),
                &[&user_id, &difficulty.key(), &puzzle_date, &mistakes],
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
                "SELECT * FROM chess_puzzle_daily_wins
                 WHERE user_id = $1 AND difficulty_key = $2 AND puzzle_date = $3",
                &[&user_id, &difficulty_key, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}
