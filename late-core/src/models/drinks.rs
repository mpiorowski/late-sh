//! Per-user tavern drink tally backing the clubhouse drunkenness glow.
//!
//! `drunk_points` is the raw buzz recorded at `last_drink_at` (chips spent on
//! drinks, capped at [`MAX_DRUNK_POINTS`]). Nothing ever writes a sober-up:
//! readers apply [`decayed_points`] against elapsed wall-clock time, so a user
//! dries out on their own and the row only changes when they buy again.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Client;
use uuid::Uuid;

/// Bounds on what the bartender may charge for a single pour.
pub const DRINK_PRICE_MIN: i64 = 100;
pub const DRINK_PRICE_MAX: i64 = 2_000;
/// How fast the buzz wears off, in drunk points (= chips) per hour.
pub const DRUNK_DECAY_PER_HOUR: i64 = 300;
/// Hard cap on stored points so one binge can't glow for days. At the decay
/// rate above a maxed-out patron is fully sober in 20 hours.
pub const MAX_DRUNK_POINTS: i64 = 6_000;

/// Level thresholds on effective (decayed) points. Level 0 renders nothing.
const DRUNK_LEVEL_THRESHOLDS: [i64; 4] = [1, 500, 1_000, 2_000];

/// Effective points after `elapsed_seconds` of sobering up.
pub fn decayed_points(points: i64, elapsed_seconds: i64) -> i64 {
    if points <= 0 {
        return 0;
    }
    let decay = elapsed_seconds.max(0) * DRUNK_DECAY_PER_HOUR / 3600;
    (points - decay).max(0)
}

/// Bucket effective points into a render level 0 (sober) through 4 (wasted).
pub fn drunk_level(effective_points: i64) -> u8 {
    DRUNK_LEVEL_THRESHOLDS
        .iter()
        .filter(|threshold| effective_points >= **threshold)
        .count() as u8
}

#[derive(Debug, Clone)]
pub struct UserDrinks {
    pub user_id: Uuid,
    pub drunk_points: i64,
    pub lifetime_spent: i64,
    pub drink_count: i64,
    pub last_drink_at: DateTime<Utc>,
}

impl From<tokio_postgres::Row> for UserDrinks {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            user_id: row.get("user_id"),
            drunk_points: row.get("drunk_points"),
            lifetime_spent: row.get("lifetime_spent"),
            drink_count: row.get("drink_count"),
            last_drink_at: row.get("last_drink_at"),
        }
    }
}

impl UserDrinks {
    /// Points remaining right now, after sobering up since the last drink.
    pub fn effective_points(&self, now: DateTime<Utc>) -> i64 {
        decayed_points(self.drunk_points, (now - self.last_drink_at).num_seconds())
    }

    /// Render level 0-4 right now.
    pub fn level(&self, now: DateTime<Utc>) -> u8 {
        drunk_level(self.effective_points(now))
    }

    /// Record a paid drink: decay the stored buzz to now, add the price, cap,
    /// and bump the permanent tallies. One statement, so concurrent buys from
    /// two sessions can't double-count the decay window.
    pub async fn record_purchase(
        client: &impl GenericClient,
        user_id: Uuid,
        price: i64,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO user_drinks
                    (user_id, drunk_points, lifetime_spent, drink_count, last_drink_at)
                 VALUES ($1, LEAST($2, $4), $2, 1, current_timestamp)
                 ON CONFLICT (user_id) DO UPDATE SET
                    drunk_points = LEAST(
                        GREATEST(
                            user_drinks.drunk_points
                                - (EXTRACT(EPOCH FROM (current_timestamp - user_drinks.last_drink_at))::bigint * $3 / 3600),
                            0
                        ) + $2,
                        $4
                    ),
                    lifetime_spent = user_drinks.lifetime_spent + $2,
                    drink_count = user_drinks.drink_count + 1,
                    last_drink_at = current_timestamp,
                    updated = current_timestamp
                 RETURNING *",
                &[&user_id, &price, &DRUNK_DECAY_PER_HOUR, &MAX_DRUNK_POINTS],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn find(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM user_drinks WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Rows that can still be drunk right now: anything that drank recently
    /// enough that the cap hasn't fully decayed. Callers compute per-user
    /// levels from these with [`UserDrinks::level`].
    pub async fn all_active(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM user_drinks
                 WHERE drunk_points > 0
                   AND last_drink_at > current_timestamp - interval '24 hours'",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Current drunk levels (only levels > 0) for all recently-drinking users.
    pub async fn active_levels(client: &Client, now: DateTime<Utc>) -> Result<HashMap<Uuid, u8>> {
        Ok(Self::all_active(client)
            .await?
            .into_iter()
            .filter_map(|drinks| {
                let level = drinks.level(now);
                (level > 0).then_some((drinks.user_id, level))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decayed_points_wears_off_linearly() {
        assert_eq!(decayed_points(600, 0), 600);
        assert_eq!(decayed_points(600, 3600), 300);
        assert_eq!(decayed_points(600, 7200), 0);
        assert_eq!(decayed_points(600, 36000), 0);
    }

    #[test]
    fn decayed_points_handles_edge_inputs() {
        assert_eq!(decayed_points(0, 3600), 0);
        assert_eq!(decayed_points(-5, 0), 0);
        // Clock skew: a last_drink_at in the future never inflates the buzz.
        assert_eq!(decayed_points(600, -3600), 600);
    }

    #[test]
    fn drunk_level_buckets() {
        assert_eq!(drunk_level(0), 0);
        assert_eq!(drunk_level(1), 1);
        assert_eq!(drunk_level(100), 1);
        assert_eq!(drunk_level(499), 1);
        assert_eq!(drunk_level(500), 2);
        assert_eq!(drunk_level(999), 2);
        assert_eq!(drunk_level(1000), 3);
        assert_eq!(drunk_level(1999), 3);
        assert_eq!(drunk_level(2000), 4);
        assert_eq!(drunk_level(MAX_DRUNK_POINTS), 4);
    }

    #[test]
    fn max_cap_dries_out_within_a_day() {
        // The 24h window in all_active must cover the slowest sober-up.
        let hours_to_sober = MAX_DRUNK_POINTS / DRUNK_DECAY_PER_HOUR;
        assert!(hours_to_sober <= 24);
        assert_eq!(decayed_points(MAX_DRUNK_POINTS, hours_to_sober * 3600), 0);
    }

    #[test]
    fn effective_points_uses_last_drink_at() {
        let now = Utc::now();
        let drinks = UserDrinks {
            user_id: Uuid::nil(),
            drunk_points: 600,
            lifetime_spent: 600,
            drink_count: 1,
            last_drink_at: now - chrono::Duration::hours(1),
        };
        assert_eq!(drinks.effective_points(now), 300);
        assert_eq!(drinks.level(now), 1);
    }
}
