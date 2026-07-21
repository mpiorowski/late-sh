use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "pet_companions";
    user_field = user_id;
    params = PetCompanionParams;
    struct PetCompanion {
        @data
        pub user_id: Uuid,
        pub last_fed: Option<DateTime<Utc>>,
        pub last_watered: Option<DateTime<Utc>>,
        pub last_played: Option<DateTime<Utc>>,
        pub last_treated: Option<DateTime<Utc>>,
        pub adopted_at: Option<DateTime<Utc>>,
        pub name: Option<String>,
        pub species: String,
        pub care_streak_days: i32,
        pub care_streak_date: Option<NaiveDate>,
    }
}

/// Maximum length of a user-set pet name.
pub const PET_NAME_MAX_CHARS: usize = 24;

pub const PET_SPECIES_CAT: &str = "cat";
pub const PET_SPECIES_DOG: &str = "dog";

/// Life stage of the pet, derived from how many days it has existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifeStage {
    Young,
    Junior,
    Adult,
    Senior,
}

impl LifeStage {
    /// Species-aware display label for use in the modal title and elsewhere.
    pub fn label(self, species: &str) -> &'static str {
        if species == PET_SPECIES_DOG {
            match self {
                LifeStage::Young => "Puppy",
                LifeStage::Junior => "Young Dog",
                LifeStage::Adult => "Adult Dog",
                LifeStage::Senior => "Senior Dog",
            }
        } else {
            match self {
                LifeStage::Young => "Kitten",
                LifeStage::Junior => "Young Cat",
                LifeStage::Adult => "Adult",
                LifeStage::Senior => "Wise Old Cat",
            }
        }
    }

    /// Stage bucket for a given age in days. Negative inputs are treated as 0.
    pub fn from_age_days(days: i64) -> Self {
        match days.max(0) {
            0..=6 => LifeStage::Young,
            7..=29 => LifeStage::Junior,
            30..=179 => LifeStage::Adult,
            _ => LifeStage::Senior,
        }
    }
}

/// Pet age in whole days. Clamped at 0 so freshly-created or future-dated
/// rows count as "today" rather than panicking the renderer with negatives.
pub fn pet_age_days(created: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - created).num_days().max(0)
}

/// Timestamp used for pet age. Purchased pets age from adoption; pre-adoption
/// fallback states still use row creation so the UI can render sensibly.
pub fn pet_age_anchor(created: DateTime<Utc>, adopted_at: Option<DateTime<Utc>>) -> DateTime<Utc> {
    adopted_at.unwrap_or(created)
}

/// Human-readable age label that pairs naturally with a life-stage label,
/// e.g. "today", "3 days", "2 weeks", "5 months", "1 year".
pub fn pet_age_label(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let days = pet_age_days(created, now);
    if days < 1 {
        return "today".to_string();
    }
    if days < 14 {
        return if days == 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        };
    }
    if days < 30 {
        let weeks = days / 7;
        return if weeks == 1 {
            "1 week".to_string()
        } else {
            format!("{weeks} weeks")
        };
    }
    if days < 365 {
        let months = days / 30;
        return if months == 1 {
            "1 month".to_string()
        } else {
            format!("{months} months")
        };
    }
    let years = days / 365;
    if years == 1 {
        "1 year".to_string()
    } else {
        format!("{years} years")
    }
}

/// Normalise a candidate pet name. Trims surrounding whitespace, collapses
/// inner whitespace runs to a single space, caps to `PET_NAME_MAX_CHARS`
/// characters. Returns `None` when the result would be empty.
pub fn normalize_pet_name(input: &str) -> Option<String> {
    let collapsed: String = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(PET_NAME_MAX_CHARS).collect())
}

impl PetCompanion {
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO pet_companions (user_id) VALUES ($1)
                 ON CONFLICT (user_id) DO UPDATE SET updated = pet_companions.updated
                 RETURNING *",
                &[&user_id],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn touch_fed(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET last_fed = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_watered(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET last_watered = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_played(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET last_played = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn record_care_completed(
        client: &Client,
        user_id: Uuid,
        care_date: NaiveDate,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions
                 SET care_streak_days = CASE
                         WHEN care_streak_date = $2 THEN care_streak_days
                         WHEN care_streak_date = ($2::date - 1) THEN care_streak_days + 1
                         ELSE 1
                     END,
                     care_streak_date = $2,
                     updated = current_timestamp
                 WHERE user_id = $1
                   AND (care_streak_date IS NULL OR care_streak_date <= $2)",
                &[&user_id, &care_date],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_treated(client: &Client, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET last_treated = current_timestamp, updated = current_timestamp WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn set_name(client: &Client, user_id: Uuid, name: Option<&str>) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET name = $1, updated = current_timestamp WHERE user_id = $2",
                &[&name, &user_id],
            )
            .await?;
        Ok(())
    }

    pub async fn set_species(client: &Client, user_id: Uuid, species: &str) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET species = $1, updated = current_timestamp WHERE user_id = $2",
                &[&species, &user_id],
            )
            .await?;
        Ok(())
    }

    /// Life stage for this pet at the given `now`, derived from adoption when
    /// available and row creation otherwise.
    pub fn life_stage(&self, now: DateTime<Utc>) -> LifeStage {
        LifeStage::from_age_days(pet_age_days(
            pet_age_anchor(self.created, self.adopted_at),
            now,
        ))
    }
}
