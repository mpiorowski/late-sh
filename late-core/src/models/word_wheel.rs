use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::chips::Difficulty;

crate::model! {
    table = "word_wheel_daily_wheels";
    params = DailyWheelParams;
    struct DailyWheel {
        @data
        pub puzzle_date: NaiveDate,
        pub letters: String,
    }
}

crate::user_scoped_model! {
    table = "word_wheel_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub letters: String,
        pub found: Vec<String>,
    }
}

crate::user_scoped_model! {
    table = "word_wheel_daily_wins";
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

#[derive(Debug)]
pub struct WinRecord {
    pub win: DailyWin,
    pub fresh: bool,
}

impl DailyWheel {
    pub async fn find_by_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM word_wheel_daily_wheels WHERE puzzle_date = $1",
                &[&puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Take the transaction-scoped advisory lock guarding the day's claim,
    /// so concurrent `ensure_daily_wheel` callers serialize on the
    /// check-then-insert instead of racing. Released when the transaction
    /// ends, so the caller must hold an open transaction.
    pub async fn lock_daily_creation(client: &impl GenericClient) -> Result<()> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('word_wheel_daily_wheel', 0))",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Every wheel ever claimed, with the last day it was claimed for.
    pub async fn last_used(client: &impl GenericClient) -> Result<HashMap<String, NaiveDate>> {
        let rows = client
            .query(
                "SELECT letters, MAX(puzzle_date) AS last_used
                 FROM word_wheel_daily_wheels
                 GROUP BY letters",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("letters"), row.get("last_used")))
            .collect())
    }

    pub async fn insert_for_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
        letters: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO word_wheel_daily_wheels (puzzle_date, letters)
                 VALUES ($1, $2)
                 ON CONFLICT (puzzle_date) DO UPDATE SET letters = word_wheel_daily_wheels.letters
                 RETURNING *",
                &[&puzzle_date, &letters],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl Game {
    pub async fn find_for_date(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM word_wheel_games WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Store a session's words, merged with the ones already stored for the
    /// same wheel, deduplicated and alphabetical. A row for other letters
    /// is replaced outright.
    pub async fn upsert(client: &Client, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO word_wheel_games (user_id, puzzle_date, letters, found)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                   found = CASE
                     WHEN word_wheel_games.letters = $3 THEN ARRAY(
                       SELECT DISTINCT word
                       FROM unnest(word_wheel_games.found || $4) AS word
                       ORDER BY word
                     )
                     ELSE $4
                   END,
                   letters = $3,
                   updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.puzzle_date,
                    &params.letters,
                    &params.found,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl DailyWin {
    /// Record a rank reached. The first record of a (user, rank, day) is
    /// `fresh` and counts once toward `daily_win_totals`; a repeat keeps
    /// the higher score and is not fresh.
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        score: i32,
    ) -> Result<WinRecord> {
        let row = client
            .query_one(
                &format!(
                    "WITH win AS (
                         INSERT INTO word_wheel_daily_wins
                           (user_id, difficulty_key, puzzle_date, score)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT (user_id, difficulty_key, puzzle_date) DO UPDATE SET
                           score = GREATEST(word_wheel_daily_wins.score, $4),
                           updated = current_timestamp
                         RETURNING *, (xmax = 0) AS fresh_win
                     ),
                     total AS (
                         {bump}
                     )
                     SELECT * FROM win",
                    bump = super::leaderboard::bump_daily_win_total_sql(
                        super::leaderboard::DailyPuzzle::WordWheel
                    ),
                ),
                &[&user_id, &difficulty.key(), &puzzle_date, &score],
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
                "SELECT * FROM word_wheel_daily_wins
                 WHERE user_id = $1 AND difficulty_key = $2 AND puzzle_date = $3",
                &[&user_id, &difficulty_key, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}
