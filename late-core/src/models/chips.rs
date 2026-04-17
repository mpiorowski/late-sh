use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::Client;
use uuid::Uuid;

pub const BONSAI_WATER_BONUS: i64 = 200;
pub const CHIP_FLOOR: i64 = 100;

/// Map a difficulty/size key to its chip bonus.
pub fn difficulty_bonus(key: &str) -> i64 {
    match key {
        "easy" | "10x10" | "draw-1" => 50,
        "medium" | "15x15" => 100,
        "hard" | "20x20" | "draw-3" => 150,
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
                &[&user_id, &CHIP_FLOOR],
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
                "INSERT INTO user_chips (user_id, balance)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET
                   balance = user_chips.balance + $2,
                   updated = current_timestamp
                 RETURNING *",
                &[&user_id, &amount],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Deduct chips (for betting). Enforces the floor — balance never drops below CHIP_FLOOR.
    /// Returns None if the user doesn't have enough chips above the floor for the bet.
    pub async fn deduct(client: &Client, user_id: Uuid, amount: i64) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE user_chips
                 SET balance = balance - $2, updated = current_timestamp
                 WHERE user_id = $1 AND balance - $2 >= $3
                 RETURNING *",
                &[&user_id, &amount, &CHIP_FLOOR],
            )
            .await?;
        Ok(row.map(Self::from))
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
        assert_eq!(difficulty_bonus("10x10"), 50);
        assert_eq!(difficulty_bonus("15x15"), 100);
        assert_eq!(difficulty_bonus("20x20"), 150);
        assert_eq!(difficulty_bonus("draw-1"), 50);
        assert_eq!(difficulty_bonus("draw-3"), 150);
        assert_eq!(difficulty_bonus("unknown"), 50);
    }

    #[test]
    fn constants() {
        assert_eq!(BONSAI_WATER_BONUS, 200);
        assert_eq!(CHIP_FLOOR, 100);
    }
}
