use std::collections::HashMap;

use anyhow::{Result, ensure};
use chrono::NaiveDate;
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

pub const CHIP_FLOOR: i64 = 100;
pub const INITIAL_CHIP_BALANCE: i64 = 1_000;
pub const CHIP_USER_CHANGED_CHANNEL: &str = "chip_user_changed";
pub const CHIP_GIFT_SENT_REASON: &str = "chip_gift_sent";
pub const CHIP_GIFT_RECEIVED_REASON: &str = "chip_gift_received";
pub const DRINK_PURCHASE_REASON: &str = "drink_purchase";
pub const DRINK_PURCHASE_SOURCE_KIND: &str = "bartender";

pub async fn listen_for_chip_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {CHIP_USER_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// Map a difficulty key to its chip bonus.
pub fn difficulty_bonus(key: &str) -> i64 {
    match key {
        "easy" => 100,
        "medium" | "mid" | "draw-1" => 250,
        "hard" | "draw-3" => 500,
        _ => 100,
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
                 ),
                 chip_notify AS (
                    SELECT pg_notify($3, user_id::text)
                    FROM upserted
                    WHERE $2 <> 0
                 ),
                 chip_notified AS (
                    SELECT count(*) FROM chip_notify
                 )
                 SELECT upserted.*
                 FROM upserted, chip_notified",
                &[&user_id, &amount, &CHIP_USER_CHANGED_CHANNEL],
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
                 ),
                 chip_notify AS (
                    SELECT pg_notify($3, user_id::text)
                    FROM updated
                    WHERE $2 <> 0
                 ),
                 chip_notified AS (
                    SELECT count(*) FROM chip_notify
                 )
                 SELECT updated.*
                 FROM updated, chip_notified",
                &[&user_id, &amount, &CHIP_USER_CHANGED_CHANNEL],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Deduct chips for a bartender drink. Unlike [`Self::deduct`], the
    /// gift-style guard applies: the pour only succeeds when the balance
    /// stays at or above [`CHIP_FLOOR`], so the bar can't leave a user broke.
    /// The ledger row carries the drink name so the tab is auditable.
    /// Returns None if the user can't cover the drink and keep the floor.
    pub async fn deduct_for_drink(
        client: &impl GenericClient,
        user_id: Uuid,
        amount: i64,
        drink: &str,
    ) -> Result<Option<Self>> {
        ensure!(amount > 0, "drink price must be positive");
        let row = client
            .query_opt(
                "WITH updated AS (
                    UPDATE user_chips
                    SET balance = balance - $2, updated = current_timestamp
                    WHERE user_id = $1 AND balance - $2 >= $3
                    RETURNING *
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    SELECT user_id, -$2, $4, $5, $6
                    FROM updated
                    RETURNING 1
                 ),
                 chip_notify AS (
                    SELECT pg_notify($7, user_id::text)
                    FROM updated
                 ),
                 chip_notified AS (
                    SELECT count(*) FROM chip_notify
                 )
                 SELECT updated.*
                 FROM updated, chip_notified",
                &[
                    &user_id,
                    &amount,
                    &CHIP_FLOOR,
                    &DRINK_PURCHASE_REASON,
                    &DRINK_PURCHASE_SOURCE_KIND,
                    &drink,
                    &CHIP_USER_CHANGED_CHANNEL,
                ],
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
                 ),
                 chip_notify AS (
                    SELECT pg_notify($3, $1::text)
                    FROM restored
                    WHERE delta > 0
                 ),
                 chip_notified AS (
                    SELECT count(*) FROM chip_notify
                 )
                 SELECT upserted.*
                 FROM upserted, chip_notified",
                &[&user_id, &CHIP_FLOOR, &CHIP_USER_CHANGED_CHANNEL],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn transfer_gift(
        client: &impl GenericClient,
        sender_id: Uuid,
        recipient_id: Uuid,
        amount: i64,
    ) -> Result<Option<(Self, Self)>> {
        ensure!(amount > 0, "gift amount must be positive");
        ensure!(sender_id != recipient_id, "cannot gift yourself");

        // Ensure both chip rows exist as a separate statement. Data-modifying
        // CTEs in a single statement all run against one snapshot, so an inline
        // upsert would not be visible to the debit/credit UPDATEs that follow,
        // and gifting to a user without a pre-existing row would spuriously fail
        // as "insufficient chips".
        client
            .execute(
                "INSERT INTO user_chips (user_id, balance)
                 VALUES ($1, $3), ($2, $3)
                 ON CONFLICT (user_id) DO NOTHING",
                &[&sender_id, &recipient_id, &INITIAL_CHIP_BALANCE],
            )
            .await?;

        let row = client
            .query_opt(
                "WITH debited AS (
                    UPDATE user_chips
                    SET balance = balance - $3, updated = current_timestamp
                    WHERE user_id = $1 AND balance - $3 >= $4
                    RETURNING *
                 ),
                 credited AS (
                    UPDATE user_chips
                    SET balance = balance + $3, updated = current_timestamp
                    WHERE user_id = $2 AND EXISTS (SELECT 1 FROM debited)
                    RETURNING *
                 ),
                 sent_ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT user_id, -$3, $5, 'user_chips'
                    FROM debited
                    RETURNING 1
                 ),
                 received_ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT user_id, $3, $6, 'user_chips'
                    FROM credited
                    RETURNING 1
                 ),
                 notify_sender AS (
                    SELECT pg_notify($7, $1::text)
                    WHERE EXISTS (SELECT 1 FROM debited)
                 ),
                 notify_recipient AS (
                    SELECT pg_notify($7, $2::text)
                    WHERE EXISTS (SELECT 1 FROM credited)
                 )
                 SELECT
                    d.user_id AS sender_user_id,
                    d.balance AS sender_balance,
                    d.last_stipend_date AS sender_last_stipend_date,
                    c.user_id AS recipient_user_id,
                    c.balance AS recipient_balance,
                    c.last_stipend_date AS recipient_last_stipend_date
                 FROM debited d
                 JOIN credited c ON true",
                &[
                    &sender_id,
                    &recipient_id,
                    &amount,
                    &CHIP_FLOOR,
                    &CHIP_GIFT_SENT_REASON,
                    &CHIP_GIFT_RECEIVED_REASON,
                    &CHIP_USER_CHANGED_CHANNEL,
                ],
            )
            .await?;

        Ok(row.map(|row| {
            (
                Self {
                    user_id: row.get("sender_user_id"),
                    balance: row.get("sender_balance"),
                    last_stipend_date: row.get("sender_last_stipend_date"),
                },
                Self {
                    user_id: row.get("recipient_user_id"),
                    balance: row.get("recipient_balance"),
                    last_stipend_date: row.get("recipient_last_stipend_date"),
                },
            )
        }))
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
        assert_eq!(difficulty_bonus("easy"), 100);
        assert_eq!(difficulty_bonus("medium"), 250);
        assert_eq!(difficulty_bonus("mid"), 250);
        assert_eq!(difficulty_bonus("hard"), 500);
        assert_eq!(difficulty_bonus("draw-1"), 250);
        assert_eq!(difficulty_bonus("draw-3"), 500);
        assert_eq!(difficulty_bonus("unknown"), 100);
    }

    #[test]
    fn constants() {
        assert_eq!(CHIP_FLOOR, 100);
        assert_eq!(INITIAL_CHIP_BALANCE, 1_000);
    }
}
