use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::pet::{LifeStage, PetCompanion, pet_age_anchor, pet_age_label};
use uuid::Uuid;

use super::svc::PetService;

/// How the pet feels. One need, one flip, on the UTC day like the bonsai:
/// fed today and it roams its box happy, not yet and it sulks on the floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    Happy,
    Sad,
}

impl PetMood {
    pub fn label(self) -> &'static str {
        match self {
            PetMood::Happy => "happy",
            PetMood::Sad => "sad",
        }
    }

    pub fn eyes(self) -> &'static str {
        match self {
            PetMood::Happy => "^.^",
            PetMood::Sad => "T_T",
        }
    }
}

/// Outcome of a feed press. The day's meal is free; the only refusal is a
/// bowl that is already full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOutcome {
    Fed,
    AlreadyFedToday,
}

pub struct PetState {
    pub user_id: Uuid,
    pub svc: PetService,

    pub last_fed: Option<DateTime<Utc>>,

    /// User-set pet name. `None` until set via the `/petname` chat command.
    pub name: Option<String>,

    /// Species of this pet (e.g. "cat", "dog"). Drives life-stage labels.
    pub species: String,

    /// When the cat row was first created. Used as a fallback age anchor.
    pub created: DateTime<Utc>,
    /// When the user unlocked the cat companion. Drives the life-stage buckets
    /// for purchased cats.
    pub adopted_at: Option<DateTime<Utc>>,

    pub action_feedback: Option<String>,
    feedback_ticks: usize,
    animation_ticks: usize,
    /// Mood as of the previous tick, so the day-rollover flip (bowl color,
    /// pet art, where it stands) reports as a render-visible change.
    last_visual: Option<PetMood>,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl PetState {
    pub fn new(user_id: Uuid, svc: PetService, companion: PetCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            name: companion.name,
            species: companion.species,
            created: companion.created,
            adopted_at: companion.adopted_at,
            action_feedback: None,
            feedback_ticks: 0,
            animation_ticks: 0,
            last_visual: None,
        }
    }

    /// Current life stage based on how long the cat has existed.
    pub fn life_stage(&self) -> LifeStage {
        LifeStage::from_age_days(
            (Utc::now() - pet_age_anchor(self.created, self.adopted_at))
                .num_days()
                .max(0),
        )
    }

    /// Human-readable age string for display, e.g. "3 days" or "1 year".
    pub fn age_label(&self) -> String {
        pet_age_label(pet_age_anchor(self.created, self.adopted_at), Utc::now())
    }

    /// Set (or clear with `None`) the user-set pet name and persist it.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name.clone();
        self.svc.set_name_task(self.user_id, name);
    }

    /// Set the pet species and persist it.
    pub fn set_species(&mut self, species: String) {
        self.species = species.clone();
        self.svc.set_species_task(self.user_id, species);
    }

    /// Advance the pet's clocks. Returns true on state edges that need a
    /// frame even when the animation predicate is quiet: feedback expiry and
    /// the mood flip at the UTC day rollover. Pure animation cadence is the
    /// box's business (`ui::frame_changed`). `wall_tick` is the app's shared
    /// 66ms wall clock (marquee_tick): the adaptive loop ticks sparsely, so a
    /// per-call counter would slow the animation with the cadence; syncing to
    /// the wall clock keeps every speed true at any wake tier.
    pub fn tick(&mut self, wall_tick: usize) -> bool {
        let mut changed = false;
        let elapsed = wall_tick.saturating_sub(self.animation_ticks);
        self.animation_ticks = wall_tick;

        if self.action_feedback.is_some() {
            self.feedback_ticks = self.feedback_ticks.saturating_sub(elapsed);
            if self.feedback_ticks == 0 {
                self.action_feedback = None;
                changed = true;
            }
        }
        let visual = self.mood();
        if self.last_visual != Some(visual) {
            self.last_visual = Some(visual);
            changed = true;
        }
        changed
    }

    pub fn mood(&self) -> PetMood {
        mood_for(self.last_fed, Utc::now().date_naive())
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn fed_today(&self) -> bool {
        fed_on(self.last_fed, Utc::now().date_naive())
    }

    /// The day's meal: free, once per UTC day. The service pays the chips
    /// behind a DB gate; the "+chips" note lands when its event comes back
    /// (`claim_fed_chips`), never from here, since another session may
    /// already have fed today.
    pub fn feed(&mut self) -> FeedOutcome {
        let now = Utc::now();
        if fed_on(self.last_fed, now.date_naive()) {
            self.set_feedback("already fed today");
            return FeedOutcome::AlreadyFedToday;
        }
        self.last_fed = Some(now);
        self.set_feedback("fed!");
        self.svc.feed_task(self.user_id);
        FeedOutcome::Fed
    }

    /// The session's own feeding cleared the DB chip gate.
    pub fn claim_fed_chips(&mut self, chips: i64) {
        self.set_feedback(format!("fed! +{chips} chips"));
    }

    fn set_feedback(&mut self, feedback: impl Into<String>) {
        self.action_feedback = Some(feedback.into());
        self.feedback_ticks = FEEDBACK_TICKS;
    }
}

fn mood_for(last_fed: Option<DateTime<Utc>>, today: NaiveDate) -> PetMood {
    if fed_on(last_fed, today) {
        PetMood::Happy
    } else {
        PetMood::Sad
    }
}

fn fed_on(last: Option<DateTime<Utc>>, today: NaiveDate) -> bool {
    last.is_some_and(|time| time.date_naive() == today)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
