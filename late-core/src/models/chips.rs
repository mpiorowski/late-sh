use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::Client;
use uuid::Uuid;

pub const BONSAI_WATER_BONUS: i64 = 200;
pub const CHIP_FLOOR: i64 = 100;
pub const INITIAL_CHIP_BALANCE: i64 = 1_000;

/// Map a difficulty key to its chip bonus.
pub fn difficulty_bonus(key: &str) -> i64 {
    match key {
        "easy" | "draw-1" => 50,
        "medium" => 100,
        "hard" | "draw-3" => 150,
        _ => 50,
    }
}

#[derive(Debug, Clone)]
pub struct UserChips {
    pub user_id: Uuid,
    pub balance: i64,
    pub last_stipend_date: Option<NaiveDate>,
}

impl From<tokio_postgres::Row> for UserChips {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            user_id: row.get("user_id"),
            balance: row.get("balance"),
            last_stipend_date: row.get("last_stipend_date"),
        }
    }
}

impl UserChips {
    /// Ensure a chips row exists for the user. Called on SSH login.
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO user_chips (user_id, balance)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO NOTHING
                 RETURNING *",
                &[&user_id, &INITIAL_CHIP_BALANCE],
            )
            .await;
        match row {
            Ok(row) => Ok(Self::from(row)),
            Err(_) => {
                // Row already existed, fetch it
                let row = client
                    .query_one("SELECT * FROM user_chips WHERE user_id = $1", &[&user_id])
                    .await?;
                Ok(Self::from(row))
            }
        }
    }

    /// Add bonus chips (e.g. from completing a daily puzzle).
    pub async fn add_bonus(client: &Client, user_id: Uuid, amount: i64) -> Result<Self> {
        let row = client
            .query_one(
                "WITH upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = user_chips.balance + $2,
                      updated = current_timestamp
                    RETURNING *
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT user_id, $2, 'chip_credit', 'user_chips'
                    FROM upserted
                    WHERE $2 <> 0
                    RETURNING 1
                 )
                 SELECT * FROM upserted",
                &[&user_id, &amount],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Deduct chips (for betting). The floor is restored after losing settlements,
    /// so a user can wager their visible balance.
    /// Returns None if the user doesn't have enough chips for the bet.
    pub async fn deduct(client: &Client, user_id: Uuid, amount: i64) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "WITH updated AS (
                    UPDATE user_chips
                    SET balance = balance - $2, updated = current_timestamp
                    WHERE user_id = $1 AND balance >= $2
                    RETURNING *
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT user_id, -$2, 'chip_debit', 'user_chips'
                    FROM updated
                    WHERE $2 <> 0
                    RETURNING 1
                 )
                 SELECT * FROM updated",
                &[&user_id, &amount],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn restore_floor(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "WITH prior AS (
                    SELECT balance
                    FROM user_chips
                    WHERE user_id = $1
                    FOR UPDATE
                 ),
                 upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = GREATEST(user_chips.balance, $2),
                      updated = current_timestamp
                    RETURNING *
                 ),
                 restored AS (
                    SELECT GREATEST($2 - COALESCE((SELECT balance FROM prior), $2), 0)::bigint AS delta
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT $1, delta, 'floor_restore', 'user_chips'
                    FROM restored
                    WHERE delta > 0
                    RETURNING 1
                 )
                 SELECT * FROM upserted",
                &[&user_id, &CHIP_FLOOR],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// All user chip balances (for per-user lookup in leaderboard refresh).
    pub async fn all_balances(client: &Client) -> Result<HashMap<Uuid, i64>> {
        let rows = client
            .query("SELECT user_id, balance FROM user_chips", &[])
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("user_id"), row.get("balance")))
            .collect())
    }

    /// Top chip balances for the leaderboard.
    pub async fn top_balances(client: &Client, limit: i64) -> Result<Vec<ChipLeader>> {
        let rows = client
            .query(
                "SELECT u.username, c.user_id, c.balance
                 FROM user_chips c
                 JOIN users u ON u.id = c.user_id
                 WHERE c.balance > 0
                 ORDER BY c.balance DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ChipLeader {
                username: row.get("username"),
                user_id: row.get("user_id"),
                balance: row.get("balance"),
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct ChipLeader {
    pub username: String,
    pub user_id: Uuid,
    pub balance: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_bonus_mapping() {
        assert_eq!(difficulty_bonus("easy"), 50);
        assert_eq!(difficulty_bonus("medium"), 100);
        assert_eq!(difficulty_bonus("hard"), 150);
        assert_eq!(difficulty_bonus("draw-1"), 50);
        assert_eq!(difficulty_bonus("draw-3"), 150);
        assert_eq!(difficulty_bonus("unknown"), 50);
    }

    #[test]
    fn constants() {
        assert_eq!(BONSAI_WATER_BONUS, 200);
        assert_eq!(CHIP_FLOOR, 100);
        assert_eq!(INITIAL_CHIP_BALANCE, 1_000);
    }
}
