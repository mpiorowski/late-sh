use anyhow::{Result, ensure};
use chrono::NaiveDate;
use std::time::Duration;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::CHIP_USER_CHANGED_CHANNEL;

pub const GAME_PAYOUT_PERIOD_COOLDOWN: &str = "cooldown";
pub const GAME_PAYOUT_PERIOD_UTC_DAY: &str = "utc_day";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamePayoutClaim {
    pub credited: bool,
    pub balance: i64,
}

pub struct GamePayout;

impl GamePayout {
    pub async fn has_claimed_daily(
        client: &Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        payout_date: NaiveDate,
    ) -> Result<bool> {
        let period_key = payout_date.to_string();
        Self::has_claimed_period(
            client,
            user_id,
            game,
            payout_kind,
            GAME_PAYOUT_PERIOD_UTC_DAY,
            &period_key,
        )
        .await
    }

    pub async fn has_claimed_period(
        client: &Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        period_kind: &str,
        period_key: &str,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id
                 FROM game_payout_claims
                 WHERE user_id = $1
                   AND game = $2
                   AND payout_kind = $3
                   AND period_kind = $4
                   AND period_key = $5",
                &[&user_id, &game, &payout_kind, &period_kind, &period_key],
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn grant_daily(
        client: &Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        payout_date: NaiveDate,
        amount: i64,
        ledger_reason: &str,
    ) -> Result<GamePayoutClaim> {
        let period_key = payout_date.to_string();
        Self::grant_period(
            client,
            user_id,
            game,
            payout_kind,
            GAME_PAYOUT_PERIOD_UTC_DAY,
            &period_key,
            amount,
            ledger_reason,
        )
        .await
    }

    pub async fn grant_period(
        client: &Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        period_kind: &str,
        period_key: &str,
        amount: i64,
        ledger_reason: &str,
    ) -> Result<GamePayoutClaim> {
        ensure!(amount > 0, "game payout amount must be positive");

        let row = client
            .query_one(
                "WITH inserted AS (
                    INSERT INTO game_payout_claims
                      (user_id, game, payout_kind, period_kind, period_key, amount)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (user_id, game, payout_kind, period_kind, period_key) DO NOTHING
                    RETURNING id
                 ),
                 upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    SELECT $1, $6
                    WHERE EXISTS (SELECT 1 FROM inserted)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = user_chips.balance + $6,
                      updated = current_timestamp
                    RETURNING balance
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    SELECT $1, $6, $7, 'game_payout_claims', id::text
                    FROM inserted
                    RETURNING 1
                 ),
                 chip_notify AS (
                    SELECT pg_notify($8, $1::text)
                    WHERE EXISTS (SELECT 1 FROM inserted)
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
                    &game,
                    &payout_kind,
                    &period_kind,
                    &period_key,
                    &amount,
                    &ledger_reason,
                    &CHIP_USER_CHANGED_CHANNEL,
                ],
            )
            .await?;
        Ok(GamePayoutClaim {
            credited: row.get("credited"),
            balance: row.get("balance"),
        })
    }

    pub async fn grant_cooldown(
        client: &Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        cooldown: Duration,
        amount: i64,
        ledger_reason: &str,
    ) -> Result<GamePayoutClaim> {
        ensure!(amount > 0, "game payout amount must be positive");
        let cooldown_secs = cooldown.as_secs_f64();
        ensure!(
            cooldown_secs.is_finite() && cooldown_secs > 0.0,
            "game payout cooldown must be positive"
        );

        let row = client
            .query_one(
                "WITH payout_lock AS (
                    SELECT pg_advisory_xact_lock(
                      hashtextextended(
                        concat_ws(':', $1::text, $2::text, $3::text, $4::text),
                        0
                      )
                    )
                 ),
                 existing AS (
                    SELECT c.id
                    FROM game_payout_claims c, payout_lock
                    WHERE c.user_id = $1
                      AND c.game = $2
                      AND c.payout_kind = $3
                      AND c.period_kind = $4
                      AND c.created > current_timestamp - make_interval(secs => $5::double precision)
                    LIMIT 1
                 ),
                 inserted AS (
                    INSERT INTO game_payout_claims
                      (created, updated, user_id, game, payout_kind, period_kind, period_key, amount)
                    SELECT
                      clock_timestamp(),
                      clock_timestamp(),
                      $1,
                      $2,
                      $3,
                      $4,
                      to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),
                      $6
                    FROM payout_lock
                    WHERE NOT EXISTS (SELECT 1 FROM existing)
                    RETURNING id
                 ),
                 upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    SELECT $1, $6
                    WHERE EXISTS (SELECT 1 FROM inserted)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = user_chips.balance + $6,
                      updated = current_timestamp
                    RETURNING balance
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    SELECT $1, $6, $7, 'game_payout_claims', id::text
                    FROM inserted
                    RETURNING 1
                 ),
                 chip_notify AS (
                    SELECT pg_notify($8, $1::text)
                    WHERE EXISTS (SELECT 1 FROM inserted)
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
                    &game,
                    &payout_kind,
                    &GAME_PAYOUT_PERIOD_COOLDOWN,
                    &cooldown_secs,
                    &amount,
                    &ledger_reason,
                    &CHIP_USER_CHANGED_CHANNEL,
                ],
            )
            .await?;
        Ok(GamePayoutClaim {
            credited: row.get("credited"),
            balance: row.get("balance"),
        })
    }
}
