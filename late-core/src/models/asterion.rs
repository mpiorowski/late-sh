use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::CHIP_USER_CHANGED_CHANNEL;

pub const ASTERION_DAILY_ESCAPE_PAYOUT: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyEscapePayout {
    pub credited: bool,
    pub balance: i64,
}

pub struct DailyEscape;

impl DailyEscape {
    pub async fn has_claimed_today(
        client: &Client,
        user_id: Uuid,
        escape_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id
                 FROM asterion_daily_escapes
                 WHERE user_id = $1
                   AND escape_date = $2",
                &[&user_id, &escape_date],
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn grant_daily_payout(
        client: &Client,
        user_id: Uuid,
        escape_date: NaiveDate,
    ) -> Result<DailyEscapePayout> {
        let row = client
            .query_one(
                "WITH inserted AS (
                    INSERT INTO asterion_daily_escapes (user_id, escape_date)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id, escape_date) DO NOTHING
                    RETURNING user_id
                 ),
                 upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    SELECT $1, $3
                    WHERE EXISTS (SELECT 1 FROM inserted)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = user_chips.balance + $3,
                      updated = current_timestamp
                    RETURNING balance
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    SELECT $1, $3, 'asterion_escape', 'asterion_daily_escapes', $2::text
                    WHERE EXISTS (SELECT 1 FROM inserted)
                      AND $3 <> 0
                    RETURNING 1
                 ),
                 chip_notify AS (
                    SELECT pg_notify($4, $1::text)
                    WHERE EXISTS (SELECT 1 FROM inserted)
                      AND $3 <> 0
                 ),
                 chip_notified AS (
                    SELECT count(*) FROM chip_notify
                 )
                 SELECT
                   EXISTS (SELECT 1 FROM inserted) AS credited,
                   COALESCE(
                     (SELECT balance FROM upserted),
                     (SELECT balance FROM user_chips WHERE user_id = $1),
                     0
                   )::bigint AS balance
                 FROM chip_notified",
                &[
                    &user_id,
                    &escape_date,
                    &ASTERION_DAILY_ESCAPE_PAYOUT,
                    &CHIP_USER_CHANGED_CHANNEL,
                ],
            )
            .await?;
        Ok(DailyEscapePayout {
            credited: row.get("credited"),
            balance: row.get("balance"),
        })
    }
}
