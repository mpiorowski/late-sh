use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

crate::model! {
    table = "quartets_daily_puzzles";
    params = DailyWallParams;
    struct DailyWall {
        @data
        pub puzzle_date: NaiveDate,
        pub puzzle_id: i32,
    }
}

crate::user_scoped_model! {
    table = "quartets_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub puzzle_id: i32,
        pub tries: Vec<i16>,
    }
}

crate::user_scoped_model! {
    table = "quartets_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub strikes: i32,
    }
}

#[derive(Debug)]
pub struct WinRecord {
    pub win: DailyWin,
    pub fresh: bool,
}

impl DailyWall {
    pub async fn find_by_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM quartets_daily_puzzles WHERE puzzle_date = $1",
                &[&puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Take the transaction-scoped advisory lock guarding the day's claim,
    /// so concurrent `ensure_daily_wall` callers serialize on the
    /// check-then-insert instead of racing. Released when the transaction
    /// ends, so the caller must hold an open transaction.
    pub async fn lock_daily_creation(client: &impl GenericClient) -> Result<()> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('quartets_daily_wall', 0))",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Every puzzle ever claimed, with the last day it was claimed for.
    pub async fn last_used(client: &impl GenericClient) -> Result<HashMap<i32, NaiveDate>> {
        let rows = client
            .query(
                "SELECT puzzle_id, MAX(puzzle_date) AS last_used
                 FROM quartets_daily_puzzles
                 GROUP BY puzzle_id",
                &[],
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
        puzzle_id: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO quartets_daily_puzzles (puzzle_date, puzzle_id)
                 VALUES ($1, $2)
                 ON CONFLICT (puzzle_date) DO UPDATE SET puzzle_id = quartets_daily_puzzles.puzzle_id
                 RETURNING *",
                &[&puzzle_date, &puzzle_id],
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
                "SELECT * FROM quartets_games WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Store a session's tries. Tries only grow, so a stored list at least
    /// as long for the same puzzle wins and a stale session cannot hand
    /// back strikes; a row for another puzzle is replaced outright.
    pub async fn upsert(client: &Client, params: GameParams) -> Result<()> {
        client
            .execute(
                "INSERT INTO quartets_games (user_id, puzzle_date, puzzle_id, tries)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                   puzzle_id = $3,
                   tries = $4,
                   updated = current_timestamp
                 WHERE quartets_games.puzzle_id <> $3
                    OR cardinality(quartets_games.tries) < cardinality($4::SMALLINT[])",
                &[
                    &params.user_id,
                    &params.puzzle_date,
                    &params.puzzle_id,
                    &params.tries,
                ],
            )
            .await?;
        Ok(())
    }
}

impl DailyWin {
    /// Record a solved wall. The first record of a (user, day) is `fresh`
    /// and counts once toward `daily_win_totals`; a repeat keeps the fewer
    /// strikes and is not fresh.
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        strikes: i32,
    ) -> Result<WinRecord> {
        let row = client
            .query_one(
                &format!(
                    "WITH win AS (
                         INSERT INTO quartets_daily_wins (user_id, puzzle_date, strikes)
                         VALUES ($1, $2, $3)
                         ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                           strikes = LEAST(quartets_daily_wins.strikes, $3),
                           updated = current_timestamp
                         RETURNING *, (xmax = 0) AS fresh_win
                     ),
                     total AS (
                         {bump}
                     )
                     SELECT * FROM win",
                    bump = super::leaderboard::bump_daily_win_total_sql(
                        super::leaderboard::DailyPuzzle::Quartets
                    ),
                ),
                &[&user_id, &puzzle_date, &strikes],
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
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM quartets_daily_wins WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}
