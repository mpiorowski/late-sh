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
pub const DRINK_PRICE_MAX: i64 = 1_000;
/// Buzz comped to a newcomer on their first walk up to the bar. Sized to land
/// exactly on the first drunk level so the welcome round already glows.
pub const WELCOME_DRINK_POINTS: i64 = 100;
/// How fast the buzz wears off, in drunk points (= chips) per hour.
pub const DRUNK_DECAY_PER_HOUR: i64 = 300;
/// Hard cap on stored points so one binge can't glow for days. At the decay
/// rate above a maxed-out patron is fully sober in 20 hours.
pub const MAX_DRUNK_POINTS: i64 = 6_000;

/// Level thresholds on effective (decayed) points. Level 0 renders nothing;
/// level 4 ("fully wasted") lands at 3000, three top-shelf pours deep. Level 1
/// (the welcome round) glows without a word; the printed drunk label only kicks
/// in at level 2, i.e. 500 points, so a first sip stays quiet.
const DRUNK_LEVEL_THRESHOLDS: [i64; 4] = [1, 500, 1_500, 3_000];

/// Lowest level that earns a printed "(word)" label next to the name. Below it,
/// the glow carries the state on its own.
pub const DRUNK_LABEL_MIN_LEVEL: u8 = 2;

/// The top drunk level ("wasted"). The bar keeps pouring the strong stuff right
/// up to here so a patron can actually climb the ladder; only once they hit it
/// does the bartender cut them off.
pub const DRUNK_MAX_LEVEL: u8 = DRUNK_LEVEL_THRESHOLDS.len() as u8;

/// The patron's state as a single word, for the bartender prompt and the
/// clubhouse name label. Level 0 is sober; 4 is fully wasted.
pub fn drunk_level_word(level: u8) -> &'static str {
    match level {
        0 => "sober",
        1 => "tipsy",
        2 => "buzzed",
        3 => "sloshed",
        _ => "wasted",
    }
}

/// The word shown beside a drinker's name, or `None` when they are too sober
/// (below [`DRUNK_LABEL_MIN_LEVEL`]) to warrant one.
pub fn drunk_label_word(level: u8) -> Option<&'static str> {
    (level >= DRUNK_LABEL_MIN_LEVEL).then(|| drunk_level_word(level))
}

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

    /// Shared upsert behind [`Self::record_purchase`] and
    /// [`Self::record_free_pour`]: decay the stored buzz to now, add `buzz`,
    /// cap, and bump the tallies. `tab` is the chips actually charged (0 for a
    /// comped pour), tracked apart from `buzz` so a free round lights the glow
    /// without inflating `lifetime_spent`. One statement, so concurrent buys
    /// from two sessions can't double-count the decay window. Every numeric
    /// parameter is cast to bigint so Postgres never infers a `LEAST`/
    /// `GREATEST` argument as text.
    async fn record(
        client: &impl GenericClient,
        user_id: Uuid,
        buzz: i64,
        tab: i64,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO user_drinks
                    (user_id, drunk_points, lifetime_spent, drink_count, last_drink_at)
                 VALUES ($1, LEAST($2::bigint, $5::bigint), $3::bigint, 1, current_timestamp)
                 ON CONFLICT (user_id) DO UPDATE SET
                    drunk_points = LEAST(
                        GREATEST(
                            user_drinks.drunk_points
                                - (EXTRACT(EPOCH FROM (current_timestamp - user_drinks.last_drink_at))::bigint * $4::bigint / 3600),
                            0
                        ) + $2::bigint,
                        $5::bigint
                    ),
                    lifetime_spent = user_drinks.lifetime_spent + $3::bigint,
                    drink_count = user_drinks.drink_count + 1,
                    last_drink_at = current_timestamp,
                    updated = current_timestamp
                 RETURNING *",
                &[
                    &user_id,
                    &buzz,
                    &tab,
                    &DRUNK_DECAY_PER_HOUR,
                    &MAX_DRUNK_POINTS,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Record a paid drink: `price` chips become both buzz and tab.
    pub async fn record_purchase(
        client: &impl GenericClient,
        user_id: Uuid,
        price: i64,
    ) -> Result<Self> {
        Self::record(client, user_id, price, price).await
    }

    /// Comp a drink on the house: `points` of buzz with no chips charged, so
    /// `lifetime_spent` stays put. Backs the tutorial's welcome round.
    pub async fn record_free_pour(
        client: &impl GenericClient,
        user_id: Uuid,
        points: i64,
    ) -> Result<Self> {
        Self::record(client, user_id, points, 0).await
    }

    /// Record one glass of a bought round: `points` of buzz, with `tab` chips
    /// attributed to this row. The payer's own glass carries the round's full
    /// price so their `lifetime_spent` matches the chip ledger; everyone
    /// else's rides at 0.
    pub async fn record_round_pour(
        client: &impl GenericClient,
        user_id: Uuid,
        points: i64,
        tab: i64,
    ) -> Result<Self> {
        Self::record(client, user_id, points, tab).await
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
        // The welcome round lands on level 1: a glow, but no printed word yet.
        assert_eq!(drunk_level(WELCOME_DRINK_POINTS), 1);
        assert_eq!(drunk_level(499), 1);
        assert_eq!(drunk_level(500), 2);
        assert_eq!(drunk_level(1499), 2);
        assert_eq!(drunk_level(1500), 3);
        assert_eq!(drunk_level(2999), 3);
        assert_eq!(drunk_level(3000), 4);
        assert_eq!(drunk_level(MAX_DRUNK_POINTS), 4);
    }

    #[test]
    fn drunk_label_word_starts_at_level_two() {
        // Below 500 points the glow stands alone; from level 2 up a word prints.
        assert_eq!(drunk_label_word(0), None);
        assert_eq!(drunk_label_word(1), None);
        assert_eq!(drunk_label_word(drunk_level(WELCOME_DRINK_POINTS)), None);
        assert_eq!(drunk_label_word(2), Some("buzzed"));
        assert_eq!(drunk_label_word(3), Some("sloshed"));
        assert_eq!(drunk_label_word(4), Some("wasted"));
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
