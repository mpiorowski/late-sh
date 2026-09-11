use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

// `species` is one of [`PetSpecies`] and `mood` one of [`PetMood`], both as
// their `as_str` and both checked by the database (migration 181); read
// them through the typed accessors. The mood is the one the owner's session
// last inferred, written on every change so a profile can show it while
// the owner is away.
crate::user_scoped_model! {
    table = "pet_companions";
    user_field = user_id;
    params = PetCompanionParams;
    struct PetCompanion {
        @data
        pub user_id: Uuid,
        pub adopted_at: Option<DateTime<Utc>>,
        pub name: Option<String>,
        pub species: String,
        pub mood: String,
        pub mood_since: DateTime<Utc>,
    }
}

/// Maximum length of a user-set pet name.
pub const PET_NAME_MAX_CHARS: usize = 24;

/// What the pet is. Closed: the database checks the column against this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetSpecies {
    Cat,
    Dog,
    Bird,
}

impl PetSpecies {
    pub const ALL: [PetSpecies; 3] = [PetSpecies::Cat, PetSpecies::Dog, PetSpecies::Bird];

    pub fn as_str(self) -> &'static str {
        match self {
            PetSpecies::Cat => "cat",
            PetSpecies::Dog => "dog",
            PetSpecies::Bird => "bird",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cat" => Some(PetSpecies::Cat),
            "dog" => Some(PetSpecies::Dog),
            "bird" => Some(PetSpecies::Bird),
            _ => None,
        }
    }

    /// The Shop's `t` key walks this ring.
    pub fn next(self) -> Self {
        match self {
            PetSpecies::Cat => PetSpecies::Dog,
            PetSpecies::Dog => PetSpecies::Bird,
            PetSpecies::Bird => PetSpecies::Cat,
        }
    }
}

/// How the pet feels: what its owner's session has been doing, read as a
/// creature would. Ordered by precedence, first wins (`PetMood::ALL`); the
/// windows and the reading live in `late-ssh/src/app/pet/state.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    /// Just petted (a click on it).
    Purring,
    /// Its owner won something lately.
    Proud,
    /// Its owner lost something lately.
    Sulking,
    /// Its owner said something lately.
    Chatty,
    /// No key from its owner in a long while, or no owner online at all.
    Asleep,
    /// Music is on and nothing else is happening.
    Vibing,
    /// Awake, and nothing in particular going on.
    Idle,
}

impl PetMood {
    pub const ALL: [PetMood; 7] = [
        PetMood::Purring,
        PetMood::Proud,
        PetMood::Sulking,
        PetMood::Chatty,
        PetMood::Asleep,
        PetMood::Vibing,
        PetMood::Idle,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PetMood::Purring => "purring",
            PetMood::Proud => "proud",
            PetMood::Sulking => "sulking",
            PetMood::Chatty => "chatty",
            PetMood::Vibing => "vibing",
            PetMood::Asleep => "asleep",
            PetMood::Idle => "idle",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "purring" => Some(PetMood::Purring),
            "proud" => Some(PetMood::Proud),
            "sulking" => Some(PetMood::Sulking),
            "chatty" => Some(PetMood::Chatty),
            "vibing" => Some(PetMood::Vibing),
            "asleep" => Some(PetMood::Asleep),
            "idle" => Some(PetMood::Idle),
            _ => None,
        }
    }
}

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
    pub fn label(self, species: PetSpecies) -> &'static str {
        match (species, self) {
            (PetSpecies::Cat, LifeStage::Young) => "Kitten",
            (PetSpecies::Cat, LifeStage::Junior) => "Young Cat",
            (PetSpecies::Cat, LifeStage::Adult) => "Adult",
            (PetSpecies::Cat, LifeStage::Senior) => "Wise Old Cat",
            (PetSpecies::Dog, LifeStage::Young) => "Puppy",
            (PetSpecies::Dog, LifeStage::Junior) => "Young Dog",
            (PetSpecies::Dog, LifeStage::Adult) => "Adult Dog",
            (PetSpecies::Dog, LifeStage::Senior) => "Senior Dog",
            (PetSpecies::Bird, LifeStage::Young) => "Chick",
            (PetSpecies::Bird, LifeStage::Junior) => "Fledgling",
            (PetSpecies::Bird, LifeStage::Adult) => "Adult Bird",
            (PetSpecies::Bird, LifeStage::Senior) => "Old Bird",
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

    /// The row's species. The database checks the column, so an unknown
    /// value is a broken row, not a case to handle.
    pub fn species(&self) -> PetSpecies {
        PetSpecies::parse(&self.species).expect("pet species is checked by the database")
    }

    /// The row's mood, same contract as [`PetCompanion::species`].
    pub fn mood(&self) -> PetMood {
        PetMood::parse(&self.mood).expect("pet mood is checked by the database")
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

    pub async fn set_species(client: &Client, user_id: Uuid, species: PetSpecies) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET species = $1, updated = current_timestamp WHERE user_id = $2",
                &[&species.as_str(), &user_id],
            )
            .await?;
        Ok(())
    }

    /// Record the mood the owner's session inferred. `mood_since` only moves
    /// when the mood does, so a repeated write of the same mood is a no-op.
    pub async fn set_mood(client: &Client, user_id: Uuid, mood: PetMood) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions
                 SET mood = $1,
                     mood_since = CASE WHEN mood = $1 THEN mood_since ELSE current_timestamp END,
                     updated = current_timestamp
                 WHERE user_id = $2",
                &[&mood.as_str(), &user_id],
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
