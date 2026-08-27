use anyhow::{Result, ensure};
use chrono::NaiveDate;
use std::time::Duration;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::ChipMove;

pub const GAME_PAYOUT_PERIOD_COOLDOWN: &str = "cooldown";
pub const GAME_PAYOUT_PERIOD_UTC_DAY: &str = "utc_day";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamePayoutClaim {
    pub credited: bool,
    pub balance: i64,
}

pub struct GamePayout;

pub struct GamePayoutPeriodGrant<'a> {
    pub user_id: Uuid,
    pub game: &'a str,
    pub payout_kind: &'a str,
    pub period_kind: &'a str,
    pub period_key: &'a str,
    pub amount: i64,
    pub chip_move: ChipMove,
}

/// One gate in a [`GamePayoutMultiGrant`]. Every gate that passes writes its
/// own `game_payout_claims` row; one gate refusing means no rows, no chips,
/// and no ledger line at all.
#[derive(Clone, Copy, Debug)]
pub enum GamePayoutKey<'a> {
    /// Refuse when this exact `(period_kind, period_key)` already landed. The
    /// key is the caller's identity for the thing being paid for: a run, a
    /// character, a match.
    Unique {
        period_kind: &'a str,
        period_key: &'a str,
    },
    /// Refuse when any row of `period_kind` landed inside `window`. The row's
    /// key is the moment it landed, so the next grant past the window always
    /// has a free key to take.
    Cooldown {
        period_kind: &'a str,
        window: Duration,
    },
}

/// An all-or-nothing payout behind several gates at once: the Lateania crowns
/// pay once per character AND at most once a week per account, the roguelike
/// doors once per ingested run AND at most once a week.
///
/// Every row carries the full `amount`, because the table's CHECK forbids a
/// zero and a claim row records what the claim was worth. The money witness is
/// `chip_ledger`, which takes exactly one row per credited grant.
pub struct GamePayoutMultiGrant<'a> {
    pub user_id: Uuid,
    pub game: &'a str,
    pub payout_kind: &'a str,
    pub keys: &'a [GamePayoutKey<'a>],
    pub amount: i64,
    pub chip_move: ChipMove,
}

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
        chip_move: ChipMove,
    ) -> Result<GamePayoutClaim> {
        let period_key = payout_date.to_string();
        Self::grant_period(
            client,
            GamePayoutPeriodGrant {
                user_id,
                game,
                payout_kind,
                period_kind: GAME_PAYOUT_PERIOD_UTC_DAY,
                period_key: &period_key,
                amount,
                chip_move,
            },
        )
        .await
    }

    pub async fn grant_period(
        client: &Client,
        grant: GamePayoutPeriodGrant<'_>,
    ) -> Result<GamePayoutClaim> {
        let GamePayoutPeriodGrant {
            user_id,
            game,
            payout_kind,
            period_kind,
            period_key,
            amount,
            chip_move,
        } = grant;
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
                    SELECT $1, $6, $7, $8, id::text
                    FROM inserted
                 )
                 SELECT
                   EXISTS (SELECT 1 FROM inserted) AS credited,
                   COALESCE(
                     (SELECT balance FROM upserted),
                     (SELECT balance FROM user_chips WHERE user_id = $1),
                     0
                   )::bigint AS balance",
                &[
                    &user_id,
                    &game,
                    &payout_kind,
                    &period_kind,
                    &period_key,
                    &amount,
                    &chip_move.reason(),
                    &chip_move.source_kind(),
                ],
            )
            .await?;
        Ok(GamePayoutClaim {
            credited: row.get("credited"),
            balance: row.get("balance"),
        })
    }

    pub async fn grant_cooldown(
        client: &mut Client,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        cooldown: Duration,
        amount: i64,
        chip_move: ChipMove,
    ) -> Result<GamePayoutClaim> {
        ensure!(amount > 0, "game payout amount must be positive");
        let cooldown_secs = cooldown.as_secs_f64();
        ensure!(
            cooldown_secs.is_finite() && cooldown_secs > 0.0,
            "game payout cooldown must be positive"
        );

        let tx = client.transaction().await?;
        lock_payout(&tx, user_id, game, payout_kind).await?;

        let row = tx
            .query_one(
                "WITH existing AS (
                    SELECT c.id
                    FROM game_payout_claims c
                    WHERE c.user_id = $1
                      AND c.game = $2
                      AND c.payout_kind = $3
                      AND c.period_kind = $4
                      AND c.created > clock_timestamp() - make_interval(secs => $5::double precision)
                    ORDER BY c.created DESC
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
                    SELECT $1, $6, $7, $8, id::text
                    FROM inserted
                 )
                 SELECT
                   EXISTS (SELECT 1 FROM inserted) AS credited,
                   COALESCE(
                     (SELECT balance FROM upserted),
                     (SELECT balance FROM user_chips WHERE user_id = $1),
                     0
                   )::bigint AS balance",
                &[
                    &user_id,
                    &game,
                    &payout_kind,
                    &GAME_PAYOUT_PERIOD_COOLDOWN,
                    &cooldown_secs,
                    &amount,
                    &chip_move.reason(),
                    &chip_move.source_kind(),
                ],
            )
            .await?;
        let claim = GamePayoutClaim {
            credited: row.get("credited"),
            balance: row.get("balance"),
        };
        tx.commit().await?;
        Ok(claim)
    }

    /// Pay `amount` only when every gate in `keys` passes, writing one claim
    /// row per gate in the same transaction as the credit. Any gate refusing
    /// means no rows, no chips, and no ledger line, so a caller can hang a
    /// per-run identity and a per-account lockout on the same payout and get
    /// one answer.
    ///
    /// Serialized against itself and [`Self::grant_cooldown`] on the same
    /// `(user, game, payout_kind)` advisory lock, because a cooldown gate is
    /// read-then-write and two replicas landing the same milestone would
    /// otherwise both see an empty window.
    pub async fn grant_multi(
        client: &mut Client,
        grant: GamePayoutMultiGrant<'_>,
    ) -> Result<GamePayoutClaim> {
        let GamePayoutMultiGrant {
            user_id,
            game,
            payout_kind,
            keys,
            amount,
            chip_move,
        } = grant;
        ensure!(amount > 0, "game payout amount must be positive");
        ensure!(!keys.is_empty(), "game payout multi grant needs a key");
        for key in keys {
            match key {
                GamePayoutKey::Unique { .. } => {}
                GamePayoutKey::Cooldown { window, .. } => {
                    let secs = window.as_secs_f64();
                    ensure!(
                        secs.is_finite() && secs > 0.0,
                        "game payout cooldown must be positive"
                    );
                }
            }
        }

        let tx = client.transaction().await?;
        lock_payout(&tx, user_id, game, payout_kind).await?;

        for key in keys {
            let blocked = match key {
                GamePayoutKey::Unique {
                    period_kind,
                    period_key,
                } => tx
                    .query_opt(
                        "SELECT id
                         FROM game_payout_claims
                         WHERE user_id = $1
                           AND game = $2
                           AND payout_kind = $3
                           AND period_kind = $4
                           AND period_key = $5",
                        &[&user_id, &game, &payout_kind, period_kind, period_key],
                    )
                    .await?
                    .is_some(),
                GamePayoutKey::Cooldown {
                    period_kind,
                    window,
                } => tx
                    .query_opt(
                        "SELECT id
                         FROM game_payout_claims
                         WHERE user_id = $1
                           AND game = $2
                           AND payout_kind = $3
                           AND period_kind = $4
                           AND created > clock_timestamp()
                                         - make_interval(secs => $5::double precision)
                         LIMIT 1",
                        &[
                            &user_id,
                            &game,
                            &payout_kind,
                            period_kind,
                            &window.as_secs_f64(),
                        ],
                    )
                    .await?
                    .is_some(),
            };
            if blocked {
                let balance = balance_of(&tx, user_id).await?;
                tx.rollback().await?;
                return Ok(GamePayoutClaim {
                    credited: false,
                    balance,
                });
            }
        }

        // Every gate passed, so every row is a plain INSERT: the gates above
        // already proved there is nothing to conflict with, and the advisory
        // lock holds until commit.
        let mut claim_ids = Vec::with_capacity(keys.len());
        for key in keys {
            let row = match key {
                GamePayoutKey::Unique {
                    period_kind,
                    period_key,
                } => {
                    tx.query_one(
                        "INSERT INTO game_payout_claims
                           (created, updated, user_id, game, payout_kind, period_kind, period_key, amount)
                         VALUES (clock_timestamp(), clock_timestamp(), $1, $2, $3, $4, $5, $6)
                         RETURNING id",
                        &[&user_id, &game, &payout_kind, period_kind, period_key, &amount],
                    )
                    .await?
                }
                GamePayoutKey::Cooldown { period_kind, .. } => {
                    tx.query_one(
                        "INSERT INTO game_payout_claims
                           (created, updated, user_id, game, payout_kind, period_kind, period_key, amount)
                         VALUES (
                           clock_timestamp(),
                           clock_timestamp(),
                           $1,
                           $2,
                           $3,
                           $4,
                           to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),
                           $5
                         )
                         RETURNING id",
                        &[&user_id, &game, &payout_kind, period_kind, &amount],
                    )
                    .await?
                }
            };
            claim_ids.push(row.get::<_, Uuid>("id"));
        }

        // One credit and one ledger row for the whole grant, sourced on the
        // first gate's claim: the money moved once, whatever the gates cost.
        let source_ref = claim_ids[0].to_string();
        let row = tx
            .query_one(
                "WITH upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = user_chips.balance + $2,
                      updated = current_timestamp
                    RETURNING balance
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    VALUES ($1, $2, $3, $4, $5)
                 )
                 SELECT (SELECT balance FROM upserted)::bigint AS balance",
                &[
                    &user_id,
                    &amount,
                    &chip_move.reason(),
                    &chip_move.source_kind(),
                    &source_ref,
                ],
            )
            .await?;
        let claim = GamePayoutClaim {
            credited: true,
            balance: row.get("balance"),
        };
        tx.commit().await?;
        Ok(claim)
    }
}

/// Serialize every read-then-write payout path for one `(user, game,
/// payout_kind)`. Shared by [`GamePayout::grant_cooldown`] and
/// [`GamePayout::grant_multi`] so the two never race each other on the same
/// payout, and taken before any claim row is read.
async fn lock_payout(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    game: &str,
    payout_kind: &str,
) -> Result<()> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
           hashtextextended(
             concat_ws(':', ($1::uuid)::text, $2::text, $3::text, $4::text),
             0
           )
         )",
        &[&user_id, &game, &payout_kind, &GAME_PAYOUT_PERIOD_COOLDOWN],
    )
    .await?;
    Ok(())
}

async fn balance_of(tx: &tokio_postgres::Transaction<'_>, user_id: Uuid) -> Result<i64> {
    let row = tx
        .query_one(
            "SELECT COALESCE(
               (SELECT balance FROM user_chips WHERE user_id = $1),
               0
             )::bigint AS balance",
            &[&user_id],
        )
        .await?;
    Ok(row.get("balance"))
}
