use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, GenericClient};
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
        // The last UTC day the owner petted it: the daily chip gate.
        pub last_petted: Option<NaiveDate>,
        // The owner's switch: false puts the pet away and it keeps everything.
        pub enabled: bool,
        // The growth, written only by `PetCompanion::store` under
        // `PetCompanion::lock`. Read the closed sets through the typed
        // accessors (`form`, `stage`, `tricks`, `learning`).
        pub days_together: i32,
        pub last_day: Option<NaiveDate>,
        pub stage: String,
        // The drawings picked on the way up, in order; read with `stage`
        // through `form`.
        pub looks: Vec<String>,
        pub bond_days: i32,
        pub last_bond_day: Option<NaiveDate>,
        pub tricks: Vec<String>,
        pub learning: Option<String>,
        pub learning_days: i32,
        pub last_practice: Option<NaiveDate>,
        // Visits made after this are the ones the owner has not looked at.
        pub visits_seen_at: DateTime<Utc>,
        pub revision: i64,
    }
}

/// Carries the owner's user id as its payload: the row is re-read by
/// whoever cares, never trusted from a serialized copy.
pub const PET_CHANGED_CHANNEL: &str = "pet_changed";

/// Maximum length of a user-set pet name.
pub const PET_NAME_MAX_CHARS: usize = 24;

/// What a petting came to on the pet's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetDay {
    /// The first pet of the UTC day, now stamped: the daily bonus is due.
    FirstToday,
    AlreadyToday,
    /// Put away: only a session that has not heard yet can still click it.
    Away,
}

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

/// How grown the pet is. Closed and ordered: the database checks the column
/// against this list, and a pet only ever moves to the next one. The days
/// each asks for and the rules of moving live in
/// `late-ssh/src/app/pet/growth.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Egg,
    Baby,
    Young,
    Adult,
    Elder,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::Egg,
        Stage::Baby,
        Stage::Young,
        Stage::Adult,
        Stage::Elder,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Egg => "egg",
            Stage::Baby => "baby",
            Stage::Young => "young",
            Stage::Adult => "adult",
            Stage::Elder => "elder",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Stage::ALL.into_iter().find(|stage| stage.as_str() == value)
    }

    /// The stage after this one, `None` for an elder.
    pub fn next(self) -> Option<Self> {
        match self {
            Stage::Egg => Some(Stage::Baby),
            Stage::Baby => Some(Stage::Young),
            Stage::Young => Some(Stage::Adult),
            Stage::Adult => Some(Stage::Elder),
            Stage::Elder => None,
        }
    }
}

/// One of the two drawings offered for a part when a pet grows. What each
/// draws depends on the animal and the part: `late-ssh/src/app/pet/ui.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    One,
    Two,
}

impl Pick {
    pub const ALL: [Pick; 2] = [Pick::One, Pick::Two];

    pub fn as_str(self) -> &'static str {
        match self {
            Pick::One => "one",
            Pick::Two => "two",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Pick::ALL.into_iter().find(|pick| pick.as_str() == value)
    }
}

/// A pet's stage together with the drawings picked on the way to it: the
/// top (ears or crest) on becoming young, the feet on becoming adult, the
/// face on becoming elder. A stage cannot be held without its picks, and
/// the database checks the two columns agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Egg,
    Baby,
    Young { top: Pick },
    Adult { top: Pick, feet: Pick },
    Elder { top: Pick, feet: Pick, face: Pick },
}

impl Form {
    pub fn stage(self) -> Stage {
        match self {
            Form::Egg => Stage::Egg,
            Form::Baby => Stage::Baby,
            Form::Young { .. } => Stage::Young,
            Form::Adult { .. } => Stage::Adult,
            Form::Elder { .. } => Stage::Elder,
        }
    }

    /// The picks made so far, in the order they were made.
    pub fn picks(self) -> Vec<Pick> {
        match self {
            Form::Egg | Form::Baby => Vec::new(),
            Form::Young { top } => vec![top],
            Form::Adult { top, feet } => vec![top, feet],
            Form::Elder { top, feet, face } => vec![top, feet, face],
        }
    }

    /// The form a stage and its picks make, `None` when they do not fit.
    pub fn from_parts(stage: Stage, picks: &[Pick]) -> Option<Self> {
        match (stage, picks) {
            (Stage::Egg, []) => Some(Form::Egg),
            (Stage::Baby, []) => Some(Form::Baby),
            (Stage::Young, &[top]) => Some(Form::Young { top }),
            (Stage::Adult, &[top, feet]) => Some(Form::Adult { top, feet }),
            (Stage::Elder, &[top, feet, face]) => Some(Form::Elder { top, feet, face }),
            (Stage::Egg | Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder, _) => None,
        }
    }

    /// The form one stage on, drawn with `pick` for the part that stage
    /// adds. `None` for an egg (it hatches, as an animal) and an elder.
    pub fn grown(self, pick: Pick) -> Option<Self> {
        match self {
            Form::Egg | Form::Elder { .. } => None,
            Form::Baby => Some(Form::Young { top: pick }),
            Form::Young { top } => Some(Form::Adult { top, feet: pick }),
            Form::Adult { top, feet } => Some(Form::Elder {
                top,
                feet,
                face: pick,
            }),
        }
    }
}

/// What a treat was earned by: one kind per family of the house's games.
/// Closed: the database checks `pet_treats.kind` against this list. The mapping from a game lives in
/// `late-ssh/src/app/pet/growth.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TreatKind {
    /// A won Arcade board, daily or personal.
    Puzzle,
    /// A finished score run.
    Arcade,
    /// A won match against another member.
    Match,
    /// A win at a house table.
    Table,
    /// Ten active minutes in a door, or a door's ending.
    Door,
}

impl TreatKind {
    pub const ALL: [TreatKind; 5] = [
        TreatKind::Puzzle,
        TreatKind::Arcade,
        TreatKind::Match,
        TreatKind::Table,
        TreatKind::Door,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TreatKind::Puzzle => "puzzle",
            TreatKind::Arcade => "arcade",
            TreatKind::Match => "match",
            TreatKind::Table => "table",
            TreatKind::Door => "door",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        TreatKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }
}

/// A trick a pet can be taught. Closed: the database checks
/// `pet_companions.tricks` and `learning` against this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trick {
    Spin,
    Bow,
    Hop,
    Dance,
}

impl Trick {
    pub const ALL: [Trick; 4] = [Trick::Spin, Trick::Bow, Trick::Hop, Trick::Dance];

    pub fn as_str(self) -> &'static str {
        match self {
            Trick::Spin => "spin",
            Trick::Bow => "bow",
            Trick::Hop => "hop",
            Trick::Dance => "dance",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Trick::ALL.into_iter().find(|trick| trick.as_str() == value)
    }
}

/// How someone else came to a pet. Closed: the database checks
/// `pet_visits.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitKind {
    /// A pat from the pet's profile.
    Pat,
    /// The visitor's own pet came calling.
    Call,
}

impl VisitKind {
    pub const ALL: [VisitKind; 2] = [VisitKind::Pat, VisitKind::Call];

    pub fn as_str(self) -> &'static str {
        match self {
            VisitKind::Pat => "pat",
            VisitKind::Call => "call",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        VisitKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }
}

/// Everything `PetCompanion::store` writes: the growth and nothing else.
/// The name, the species, the mood, the switch and the daily pat are their
/// own single-column writes and never ride a stored copy. No revision: the
/// row owns that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetGrowthWrite {
    pub user_id: Uuid,
    pub days_together: i32,
    pub last_day: Option<NaiveDate>,
    pub form: Form,
    pub bond_days: i32,
    pub last_bond_day: Option<NaiveDate>,
    pub tricks: Vec<Trick>,
    pub learning: Option<Trick>,
    pub learning_days: i32,
    pub last_practice: Option<NaiveDate>,
}

/// The last treat the pet was given, and the UTC day it ate it. The day it
/// names is the day the pet goes by the treat's title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fed {
    pub kind: TreatKind,
    pub on: NaiveDate,
}

/// What asking for a treat came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreatEarned {
    /// The day's treat waits in the pouch.
    Earned,
    /// The day's treat was already earned, of whatever kind.
    AlreadyToday,
    /// An earlier treat still waits to be given.
    Waiting,
}

/// A visit the owner has not looked at yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetVisitor {
    pub visitor_id: Uuid,
    pub username: String,
    pub kind: VisitKind,
    pub at: DateTime<Utc>,
}

/// A pet staying with another: the caller's pet, as it looks now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetGuest {
    pub visitor_id: Uuid,
    /// Its owner's username.
    pub username: String,
    pub species: PetSpecies,
    pub form: Form,
    pub name: Option<String>,
    /// When it came.
    pub since: DateTime<Utc>,
}

/// Where a user's own pet went calling: its host's username, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetStay {
    pub host: String,
    pub since: DateTime<Utc>,
}

/// The `kind` column of a `pet_treats` row.
fn treat_kind(row: &tokio_postgres::Row) -> TreatKind {
    TreatKind::parse(row.get("kind")).expect("treat kind is checked by the database")
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

    /// What the pet is: picked at the hatch, changed by its owner after.
    /// Call it only while holding `lock`; it counts as a stored write
    /// (`revision`, like `touch`). Returns the row as it stands.
    pub async fn set_species(
        client: &impl GenericClient,
        user_id: Uuid,
        species: PetSpecies,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE pet_companions
                 SET species = $2, revision = revision + 1, updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[&user_id, &species.as_str()],
            )
            .await
            .context("setting the pet's species")?;
        Ok(Self::from(row))
    }

    /// Bring the pet out or put it away.
    pub async fn set_enabled(client: &Client, user_id: Uuid, enabled: bool) -> Result<()> {
        client
            .execute(
                "UPDATE pet_companions SET enabled = $1, updated = current_timestamp WHERE user_id = $2",
                &[&enabled, &user_id],
            )
            .await?;
        Ok(())
    }

    /// Stamp today's petting of a pet that is out. The `UPDATE` is the one
    /// witness the daily chip bonus needs, atomic however many sessions or
    /// clicks race for it, and it is what refuses a pet put away; the read
    /// after it only names why nothing was stamped.
    pub async fn pet_day(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<PetDay> {
        let stamped = client
            .execute(
                "UPDATE pet_companions
                 SET last_petted = $2, updated = current_timestamp
                 WHERE user_id = $1 AND enabled AND last_petted IS DISTINCT FROM $2",
                &[&user_id, &today],
            )
            .await?;
        if stamped == 1 {
            return Ok(PetDay::FirstToday);
        }
        let row = client
            .query_one(
                "SELECT enabled FROM pet_companions WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        match row.get::<_, bool>("enabled") {
            true => Ok(PetDay::AlreadyToday),
            false => Ok(PetDay::Away),
        }
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

    /// The row's stage, same contract as [`PetCompanion::species`].
    pub fn stage(&self) -> Stage {
        Stage::parse(&self.stage).expect("pet stage is checked by the database")
    }

    /// The stage with the drawings picked on the way to it.
    pub fn form(&self) -> Form {
        let picks: Vec<Pick> = self
            .looks
            .iter()
            .map(|pick| Pick::parse(pick).expect("pet looks are checked by the database"))
            .collect();
        Form::from_parts(self.stage(), &picks)
            .expect("pet looks fit the stage, checked by the database")
    }

    /// The tricks it knows, in the order it learned them.
    pub fn tricks(&self) -> Vec<Trick> {
        self.tricks
            .iter()
            .map(|trick| Trick::parse(trick).expect("pet tricks are checked by the database"))
            .collect()
    }

    /// The trick being taught, if any.
    pub fn learning(&self) -> Option<Trick> {
        self.learning
            .as_deref()
            .map(|trick| Trick::parse(trick).expect("pet learning is checked by the database"))
    }

    /// Take the owner's row lock for the rest of the transaction. Every
    /// growth write happens behind this lock: the caller reads the row,
    /// applies one rule to it, and stores the result, so two sessions (or
    /// two replicas) acting at once run one after the other on the same
    /// truth.
    pub async fn lock(client: &impl GenericClient, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "SELECT * FROM pet_companions WHERE user_id = $1 FOR UPDATE",
                &[&user_id],
            )
            .await
            .context("locking pet companion")?;
        Ok(Self::from(row))
    }

    /// Write the growth back. Call it only while holding `lock`. `revision`
    /// is owned here: it counts stored writes.
    pub async fn store(client: &impl GenericClient, write: PetGrowthWrite) -> Result<Self> {
        let tricks: Vec<&str> = write.tricks.iter().map(|trick| trick.as_str()).collect();
        let looks: Vec<&str> = write.form.picks().into_iter().map(Pick::as_str).collect();
        let row = client
            .query_one(
                "UPDATE pet_companions
                 SET days_together = $2,
                     last_day = $3,
                     stage = $4,
                     bond_days = $5,
                     last_bond_day = $6,
                     tricks = $7,
                     learning = $8,
                     learning_days = $9,
                     last_practice = $10,
                     looks = $11,
                     revision = revision + 1,
                     updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[
                    &write.user_id,
                    &write.days_together,
                    &write.last_day,
                    &write.form.stage().as_str(),
                    &write.bond_days,
                    &write.last_bond_day,
                    &tricks,
                    &write.learning.map(Trick::as_str),
                    &write.learning_days,
                    &write.last_practice,
                    &looks,
                ],
            )
            .await
            .context("storing pet growth")?;
        Ok(Self::from(row))
    }

    /// Count a write that changed something a mirror shows outside the
    /// growth columns (the pat's day, the pouch, a visit), so every stored
    /// snapshot of a pet is ordered by `revision`. Returns the row as it
    /// stands.
    pub async fn touch(client: &impl GenericClient, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE pet_companions
                 SET revision = revision + 1, updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[&user_id],
            )
            .await
            .context("touching pet companion")?;
        Ok(Self::from(row))
    }

    /// The owner looked at who came by: visits up to now are seen. Call it
    /// only while holding `lock`; it counts as a stored write.
    pub async fn mark_visits_seen(client: &impl GenericClient, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "UPDATE pet_companions
                 SET visits_seen_at = current_timestamp,
                     revision = revision + 1,
                     updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[&user_id],
            )
            .await
            .context("marking pet visits seen")?;
        Ok(Self::from(row))
    }

    /// Tell every replica this owner's pet moved. Sent inside the writing
    /// transaction, so it is delivered on commit and never for a rollback.
    pub async fn notify_changed(client: &impl GenericClient, user_id: Uuid) -> Result<()> {
        client
            .execute(
                "SELECT pg_notify($1, $2)",
                &[&PET_CHANGED_CHANNEL, &user_id.to_string()],
            )
            .await
            .context("notifying pet change")?;
        Ok(())
    }

    /// Ask for the treat of `today`, earned in `kind`'s family of games.
    /// The insert is the claim: the primary key refuses a second treat this
    /// UTC day, whatever its kind, and the partial unique index refuses one
    /// while another still waits. The read after it only names why nothing
    /// was earned.
    pub async fn earn_treat(
        client: &impl GenericClient,
        user_id: Uuid,
        kind: TreatKind,
        today: NaiveDate,
    ) -> Result<TreatEarned> {
        let earned = client
            .execute(
                "INSERT INTO pet_treats (user_id, kind, earned_on)
                 VALUES ($1, $2, $3)
                 ON CONFLICT DO NOTHING",
                &[&user_id, &kind.as_str(), &today],
            )
            .await
            .context("earning a pet treat")?;
        if earned == 1 {
            return Ok(TreatEarned::Earned);
        }
        match Self::waiting_treat(client, user_id).await? {
            Some(_) => Ok(TreatEarned::Waiting),
            None => Ok(TreatEarned::AlreadyToday),
        }
    }

    /// Give the treat waiting in the owner's pouch. `None` when none waits.
    pub async fn give_treat(
        client: &impl GenericClient,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<Option<TreatKind>> {
        let given = client
            .query_opt(
                "UPDATE pet_treats
                 SET given_on = $2
                 WHERE user_id = $1 AND given_on IS NULL
                 RETURNING kind",
                &[&user_id, &today],
            )
            .await
            .context("giving a pet treat")?;
        Ok(given.map(|row| treat_kind(&row)))
    }

    /// The treat waiting in the owner's pouch, if one does.
    pub async fn waiting_treat(
        client: &impl GenericClient,
        user_id: Uuid,
    ) -> Result<Option<TreatKind>> {
        let waiting = client
            .query_opt(
                "SELECT kind FROM pet_treats WHERE user_id = $1 AND given_on IS NULL",
                &[&user_id],
            )
            .await
            .context("reading the waiting pet treat")?;
        Ok(waiting.map(|row| treat_kind(&row)))
    }

    /// The last treat the owner gave, and the day it was eaten.
    pub async fn last_fed(client: &impl GenericClient, user_id: Uuid) -> Result<Option<Fed>> {
        let fed = client
            .query_opt(
                "SELECT kind, given_on FROM pet_treats
                 WHERE user_id = $1 AND given_on IS NOT NULL
                 ORDER BY given_on DESC, earned_on DESC
                 LIMIT 1",
                &[&user_id],
            )
            .await
            .context("reading the last pet treat given")?;
        Ok(fed.map(|row| Fed {
            kind: treat_kind(&row),
            on: row.get("given_on"),
        }))
    }

    /// Claim a visit of `kind` by `visitor_id` to `owner_id`'s pet on
    /// `day`. The insert is the claim: once per visitor, pet, kind and day,
    /// and a pet goes calling once a day. False when it was already made.
    pub async fn claim_visit(
        client: &impl GenericClient,
        owner_id: Uuid,
        visitor_id: Uuid,
        kind: VisitKind,
        day: NaiveDate,
    ) -> Result<bool> {
        let claimed = client
            .execute(
                "INSERT INTO pet_visits (owner_id, visitor_id, day, kind)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT DO NOTHING",
                &[&owner_id, &visitor_id, &day, &kind.as_str()],
            )
            .await
            .context("claiming a pet visit")?;
        Ok(claimed == 1)
    }

    /// The visits to `owner_id`'s pet made after `since`, newest first.
    pub async fn visitors_since(
        client: &impl GenericClient,
        owner_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<PetVisitor>> {
        let rows = client
            .query(
                "SELECT v.visitor_id, u.username, v.kind, v.created
                 FROM pet_visits v
                 JOIN users u ON u.id = v.visitor_id
                 WHERE v.owner_id = $1 AND v.created > $2
                 ORDER BY v.created DESC
                 LIMIT 20",
                &[&owner_id, &since],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|row| PetVisitor {
                visitor_id: row.get("visitor_id"),
                username: row.get("username"),
                kind: VisitKind::parse(row.get("kind"))
                    .expect("visit kind is checked by the database"),
                at: row.get("created"),
            })
            .collect())
    }

    /// The pets that came calling on `owner_id`'s after `since`, newest
    /// first, each as it looks now. One that has gone back into its egg or
    /// been put away since is not a guest any more.
    pub async fn guests_since(
        client: &impl GenericClient,
        owner_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<PetGuest>> {
        let rows = client
            .query(
                "SELECT v.visitor_id, u.username, v.created,
                        p.species, p.stage, p.looks, p.name
                 FROM pet_visits v
                 JOIN users u ON u.id = v.visitor_id
                 JOIN pet_companions p ON p.user_id = v.visitor_id
                 WHERE v.owner_id = $1 AND v.kind = 'call' AND v.created > $2
                   AND p.enabled AND p.stage <> 'egg'
                 ORDER BY v.created DESC",
                &[&owner_id, &since],
            )
            .await
            .context("reading a pet's guests")?;
        Ok(rows
            .iter()
            .map(|row| {
                let picks: Vec<Pick> = row
                    .get::<_, Vec<String>>("looks")
                    .iter()
                    .map(|pick| Pick::parse(pick).expect("pet looks are checked by the database"))
                    .collect();
                let stage =
                    Stage::parse(row.get("stage")).expect("pet stage is checked by the database");
                PetGuest {
                    visitor_id: row.get("visitor_id"),
                    username: row.get("username"),
                    species: PetSpecies::parse(row.get("species"))
                        .expect("pet species is checked by the database"),
                    form: Form::from_parts(stage, &picks)
                        .expect("pet looks fit the stage, checked by the database"),
                    name: row.get("name"),
                    since: row.get("created"),
                }
            })
            .collect())
    }

    /// Where `visitor_id`'s pet last went calling after `since`, if it did.
    pub async fn stay_since(
        client: &impl GenericClient,
        visitor_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Option<PetStay>> {
        let stay = client
            .query_opt(
                "SELECT u.username, v.created
                 FROM pet_visits v
                 JOIN users u ON u.id = v.owner_id
                 WHERE v.visitor_id = $1 AND v.kind = 'call' AND v.created > $2
                 ORDER BY v.created DESC
                 LIMIT 1",
                &[&visitor_id, &since],
            )
            .await
            .context("reading where a pet went calling")?;
        Ok(stay.map(|row| PetStay {
            host: row.get("username"),
            since: row.get("created"),
        }))
    }
}
