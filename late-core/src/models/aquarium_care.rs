//! The aquarium's care row: the owner's feeding and what it has earned or
//! cost the tank. Hunger is not stored; it is "not fed today (UTC)", read
//! off `last_fed`. The row also carries the feeding streak (a fry hatches
//! at `CARE_DAYS` straight fed days), the starvation deaths already settled
//! since the last meal (one fish per `CARE_DAYS` unfed days), the fry
//! still swimming small, and the sprout clock: every `SPROUT_EVERY_DAYS` a
//! sprout comes up on the floor, and one the owner has not cut within
//! `SPROUT_DAYS` roots as a plant.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use super::{aquarium_shield::AquariumShield, marketplace::FishStock};

/// Both care clocks run on the same fourteen days: that many straight fed
/// days hatch a fry, that many unfed days starve a fish.
pub const CARE_DAYS: u32 = 14;
/// How long a fry swims as a small sprite before it is drawn full size.
pub const FRY_DAYS: u32 = 7;
/// Days between one sprout coming up and the next, whatever became of it.
pub const SPROUT_EVERY_DAYS: u32 = 14;
/// How long a sprout can be cut before it roots as a plant.
pub const SPROUT_DAYS: u32 = 7;
/// The price every breeding and starvation weight is measured against: a
/// fish at this price weighs 1, a fish at a tenth of it weighs 10.
pub const WEIGHT_PRICE_CHIPS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquariumCare {
    pub last_fed: DateTime<Utc>,
    pub streak: i32,
    pub deaths_settled: i32,
    pub fry_creature: Option<String>,
    pub fry_born: Option<NaiveDate>,
    /// The day the sprout now on the floor came up; `None` when the floor
    /// is bare (never sprouted, cut, or rooted).
    pub sprout_born: Option<NaiveDate>,
    /// The day the next sprout comes up.
    pub next_sprout: NaiveDate,
}

impl From<tokio_postgres::Row> for AquariumCare {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            last_fed: row.get("last_fed"),
            streak: row.get("streak"),
            deaths_settled: row.get("deaths_settled"),
            fry_creature: row.get("fry_creature"),
            fry_born: row.get("fry_born"),
            sprout_born: row.get("sprout_born"),
            next_sprout: row.get("next_sprout"),
        }
    }
}

impl AquariumCare {
    /// The care row, `None` for a tank that has no clock running yet.
    pub async fn load(client: &impl GenericClient, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT last_fed, streak, deaths_settled, fry_creature, fry_born,
                        sprout_born, next_sprout
                 FROM user_aquarium_care
                 WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// The care row locked for the rest of the transaction, so two sessions
    /// settling the same tank at once take turns instead of both killing.
    pub async fn lock(client: &impl GenericClient, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT last_fed, streak, deaths_settled, fry_creature, fry_born,
                        sprout_born, next_sprout
                 FROM user_aquarium_care
                 WHERE user_id = $1
                 FOR UPDATE",
                &[&user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Start the clock for a tank owner who has never fed: the row is
    /// created as if the last meal was yesterday, so the tank is hungry now
    /// and the first fish is at stake `CARE_DAYS` from today. An existing row
    /// is left alone.
    pub async fn ensure(client: &impl GenericClient, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO user_aquarium_care (user_id, last_fed)
                 VALUES ($1, current_timestamp - interval '1 day')
                 ON CONFLICT (user_id) DO NOTHING",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    /// A tank just bought comes with its first sprout: the row starts like
    /// `ensure` (hungry now, the first fish at stake `CARE_DAYS` out) with a
    /// sprout up today and the next booked `SPROUT_EVERY_DAYS` out. Called
    /// inside the purchase transaction, before the welcome fry is stamped
    /// (`marketplace::welcome_aquarium_fry_in_tx`); an existing row is left
    /// alone.
    pub async fn welcome(client: &impl GenericClient, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "INSERT INTO user_aquarium_care (user_id, last_fed, sprout_born, next_sprout)
                 VALUES ($1, current_timestamp - interval '1 day', current_date,
                         current_date + $2::int)
                 ON CONFLICT (user_id) DO NOTHING",
                &[&user_id, &(SPROUT_EVERY_DAYS as i32)],
            )
            .await?;
        Ok(())
    }

    /// Stamp today's feeding. Returns the new streak when this call was the
    /// first of the UTC day and `None` when the tank already ate. The streak
    /// continues when the last meal was on or after `continues_from` (see
    /// [`streak_continues_from`]: yesterday, or earlier when a shield
    /// covered the days between) and restarts at one otherwise. The only
    /// witness the daily chip bonus and the fry need, atomic no matter how
    /// many sessions or presses race for it. A meal also closes the
    /// starvation account: the next death is `CARE_DAYS` away again.
    pub async fn feed_day(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
        continues_from: NaiveDate,
    ) -> Result<Option<i32>> {
        let row = client
            .query_opt(
                "INSERT INTO user_aquarium_care (user_id, last_fed, streak, deaths_settled)
                 VALUES ($1, current_timestamp, 1, 0)
                 ON CONFLICT (user_id) DO UPDATE
                 SET last_fed = current_timestamp,
                     streak = CASE
                         WHEN (user_aquarium_care.last_fed AT TIME ZONE 'UTC')::date >= $3::date
                         THEN user_aquarium_care.streak + 1
                         ELSE 1
                     END,
                     deaths_settled = 0,
                     updated = current_timestamp
                 WHERE (user_aquarium_care.last_fed AT TIME ZONE 'UTC')::date IS DISTINCT FROM $2::date
                 RETURNING streak",
                &[&user_id, &today, &continues_from],
            )
            .await?;
        Ok(row.map(|row| row.get("streak")))
    }

    /// Record how many starvation deaths have been taken since the last
    /// meal, after the fish are gone.
    pub async fn settle_deaths(
        client: &impl GenericClient,
        user_id: Uuid,
        deaths_settled: i32,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE user_aquarium_care
                 SET deaths_settled = $2, updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &deaths_settled],
            )
            .await?;
        Ok(())
    }

    /// Remember the fry that hatched today, so every session draws it small
    /// for its first `FRY_DAYS`.
    pub async fn set_fry(
        client: &impl GenericClient,
        user_id: Uuid,
        creature: &str,
        born: NaiveDate,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE user_aquarium_care
                 SET fry_creature = $2, fry_born = $3, updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &creature, &born],
            )
            .await?;
        Ok(())
    }

    /// A sprout comes up when one is due and the floor is bare: it is
    /// stamped `today` and the next is booked `SPROUT_EVERY_DAYS` out.
    /// Returns whether this call was the one that raised it.
    pub async fn sprout_up(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let next = today + chrono::Days::new(SPROUT_EVERY_DAYS as u64);
        let row = client
            .query_opt(
                "UPDATE user_aquarium_care
                 SET sprout_born = $2, next_sprout = $3, updated = current_timestamp
                 WHERE user_id = $1 AND sprout_born IS NULL AND next_sprout <= $2
                 RETURNING 1",
                &[&user_id, &today, &next],
            )
            .await?;
        Ok(row.is_some())
    }

    /// The sprout roots: it leaves the care row once it is `SPROUT_DAYS`
    /// old. Returns whether there was one to root; the caller grows the
    /// plant in the same transaction.
    pub async fn root_sprout(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE user_aquarium_care
                 SET sprout_born = NULL, updated = current_timestamp
                 WHERE user_id = $1 AND sprout_born IS NOT NULL
                   AND sprout_born <= $2::date - $3::int
                 RETURNING 1",
                &[&user_id, &today, &(SPROUT_DAYS as i32)],
            )
            .await?;
        Ok(row.is_some())
    }

    /// The owner cuts the sprout: gone, as long as it is still young enough
    /// to cut. Returns whether this call was the one that cut it (a rooted
    /// or already cut sprout is nothing to cut).
    pub async fn cut_sprout(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE user_aquarium_care
                 SET sprout_born = NULL, updated = current_timestamp
                 WHERE user_id = $1 AND sprout_born IS NOT NULL
                   AND sprout_born > $2::date - $3::int
                 RETURNING 1",
                &[&user_id, &today, &(SPROUT_DAYS as i32)],
            )
            .await?;
        Ok(row.is_some())
    }
}

/// Whether a sprout that came up on `born` has rooted by `today`: it can be
/// cut for `SPROUT_DAYS`, the day after that it is a plant.
pub fn sprout_rooted(born: NaiveDate, today: NaiveDate) -> bool {
    born.checked_add_days(chrono::Days::new(SPROUT_DAYS as u64))
        .is_none_or(|roots_on| today >= roots_on)
}

/// Unfed days on the clock: every UTC day after `last_fed` up to and
/// including `today`, minus the days any shield window covered. Every
/// window the owner ever bought counts, live or lapsed: a shield that ran
/// out months ago still excuses the days it covered.
pub fn dry_days(last_fed: NaiveDate, today: NaiveDate, shields: &[AquariumShield]) -> u32 {
    if today <= last_fed {
        return 0;
    }
    let mut count = 0;
    let mut day = last_fed.succ_opt();
    while let Some(current) = day
        && current <= today
    {
        if !covered(shields, current) {
            count += 1;
        }
        day = current.succ_opt();
    }
    count
}

/// The day a previous meal must fall on or after for today's meal to
/// continue the streak: yesterday, pushed back over every day a shield
/// covered, so a stretch the auto feeder minded neither grows nor breaks
/// the streak.
pub fn streak_continues_from(today: NaiveDate, shields: &[AquariumShield]) -> NaiveDate {
    let mut anchor = today;
    loop {
        let Some(previous) = anchor.pred_opt() else {
            return anchor;
        };
        anchor = previous;
        if !covered(shields, anchor) {
            return anchor;
        }
    }
}

fn covered(shields: &[AquariumShield], day: NaiveDate) -> bool {
    shields.iter().any(|shield| shield.covers_day(day))
}

/// How many fish `dry` unfed days have cost in total: one per `CARE_DAYS`.
pub fn deaths_due(dry: u32) -> u32 {
    dry / CARE_DAYS
}

/// Whether a feeding streak of this length hatches a fry today.
pub fn hatches_fry(streak: i32) -> bool {
    streak > 0 && (streak as u32).is_multiple_of(CARE_DAYS)
}

/// How much one fish of this price weighs in the breeding and starvation
/// rolls: cheap fish breed and starve often, a Bigbert rarely.
pub fn fish_weight(price_chips: i64) -> u64 {
    (WEIGHT_PRICE_CHIPS / price_chips.max(1)).max(1) as u64
}

/// Pick one swimming fish by weight (`fish_weight` per fish, so a species
/// counts once per active copy). `roll` is any random number; the same roll
/// against the same stock always picks the same species, which is what the
/// tests lean on. `None` for an empty tank.
pub fn pick_by_weight(stock: &[FishStock], roll: u64) -> Option<&FishStock> {
    let total: u64 = stock.iter().map(species_weight).sum();
    if total == 0 {
        return None;
    }
    let mut cursor = roll % total;
    for fish in stock {
        let weight = species_weight(fish);
        if cursor < weight {
            return Some(fish);
        }
        cursor -= weight;
    }
    None
}

fn species_weight(fish: &FishStock) -> u64 {
    fish_weight(fish.price_chips) * fish.active_quantity.max(0) as u64
}
