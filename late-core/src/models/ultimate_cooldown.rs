use std::time::Duration;

use anyhow::{Result, ensure};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UltimateCooldown {
    pub ultimate_id: String,
    pub remaining: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UltimateCastClaim {
    pub allowed: bool,
    pub remaining: Duration,
}

pub struct UltimateCastCooldown;

impl UltimateCastCooldown {
    pub async fn list_remaining(
        client: &Client,
        user_id: Uuid,
        cooldown: Duration,
    ) -> Result<Vec<UltimateCooldown>> {
        let cooldown_secs = checked_cooldown_secs(cooldown)?;
        let rows = client
            .query(
                "SELECT ultimate_id,
                        GREATEST(
                          0,
                          EXTRACT(EPOCH FROM (
                            last_cast_at
                            + make_interval(secs => $2::double precision)
                            - clock_timestamp()
                          ))
                        )::double precision AS remaining_secs
                 FROM ultimate_cast_cooldowns
                 WHERE user_id = $1
                   AND last_cast_at > clock_timestamp()
                        - make_interval(secs => $2::double precision)",
                &[&user_id, &cooldown_secs],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| UltimateCooldown {
                ultimate_id: row.get("ultimate_id"),
                remaining: duration_from_secs(row.get("remaining_secs")),
            })
            .collect())
    }

    pub async fn try_record_cast(
        client: &mut Client,
        user_id: Uuid,
        ultimate_id: &str,
        cooldown: Duration,
    ) -> Result<UltimateCastClaim> {
        ensure!(
            !ultimate_id.trim().is_empty(),
            "ultimate cooldown id must not be blank"
        );
        let cooldown_secs = checked_cooldown_secs(cooldown)?;
        let tx = client.transaction().await?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(
               hashtextextended(
                 concat_ws(':', $1::uuid::text, $2::text, 'ultimate_cast_cooldown'),
                 0
               )
             )",
            &[&user_id, &ultimate_id],
        )
        .await?;

        let row = tx
            .query_one(
                "WITH existing AS (
                    SELECT last_cast_at
                    FROM ultimate_cast_cooldowns
                    WHERE user_id = $1 AND ultimate_id = $2
                 ),
                 eligible AS (
                    SELECT NOT EXISTS (SELECT 1 FROM existing)
                        OR (SELECT last_cast_at FROM existing)
                           <= clock_timestamp()
                              - make_interval(secs => $3::double precision)
                        AS allowed
                 ),
                 upserted AS (
                    INSERT INTO ultimate_cast_cooldowns
                      (user_id, ultimate_id, last_cast_at)
                    SELECT $1, $2, clock_timestamp()
                    WHERE (SELECT allowed FROM eligible)
                    ON CONFLICT (user_id, ultimate_id) DO UPDATE SET
                      last_cast_at = EXCLUDED.last_cast_at
                    WHERE (SELECT allowed FROM eligible)
                    RETURNING last_cast_at
                 ),
                 current_row AS (
                    SELECT last_cast_at FROM upserted
                    UNION ALL
                    SELECT last_cast_at FROM existing
                    WHERE NOT EXISTS (SELECT 1 FROM upserted)
                    LIMIT 1
                 )
                 SELECT
                   EXISTS (SELECT 1 FROM upserted) AS allowed,
                   GREATEST(
                     0,
                     EXTRACT(EPOCH FROM (
                       (SELECT last_cast_at FROM current_row)
                       + make_interval(secs => $3::double precision)
                       - clock_timestamp()
                     ))
                   )::double precision AS remaining_secs",
                &[&user_id, &ultimate_id, &cooldown_secs],
            )
            .await?;

        let claim = UltimateCastClaim {
            allowed: row.get("allowed"),
            remaining: duration_from_secs(row.get("remaining_secs")),
        };
        tx.commit().await?;
        Ok(claim)
    }
}

fn checked_cooldown_secs(cooldown: Duration) -> Result<f64> {
    let secs = cooldown.as_secs_f64();
    ensure!(
        secs.is_finite() && secs > 0.0,
        "ultimate cooldown must be positive"
    );
    Ok(secs)
}

fn duration_from_secs(seconds: f64) -> Duration {
    if seconds.is_finite() && seconds > 0.0 {
        Duration::from_secs_f64(seconds)
    } else {
        Duration::ZERO
    }
}
