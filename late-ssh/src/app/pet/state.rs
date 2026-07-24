use chrono::{DateTime, Duration, NaiveDate, Utc};
use late_core::models::pet::{LifeStage, PetCompanion, pet_age_anchor, pet_age_label};
use uuid::Uuid;

use super::svc::PetService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    Happy,
    Content,
    Hungry,
    Thirsty,
    Sad,
}

impl PetMood {
    pub fn label(self) -> &'static str {
        match self {
            PetMood::Happy => "happy",
            PetMood::Content => "content",
            PetMood::Hungry => "hungry",
            PetMood::Thirsty => "thirsty",
            PetMood::Sad => "sad",
        }
    }

    pub fn eyes(self) -> &'static str {
        match self {
            PetMood::Happy => "^.^",
            PetMood::Content => "o.o",
            PetMood::Hungry => "o.o",
            PetMood::Thirsty => "o_o",
            PetMood::Sad => "T_T",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetNeedStatus {
    Done,
    Due,
    Overdue,
}

impl PetNeedStatus {
    pub fn label(self) -> &'static str {
        match self {
            PetNeedStatus::Done => "ok",
            PetNeedStatus::Due => "due",
            PetNeedStatus::Overdue => "late",
        }
    }

    pub fn is_missing(self) -> bool {
        self != PetNeedStatus::Done
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetNeeds {
    pub food: PetNeedStatus,
    pub water: PetNeedStatus,
}

const FOOD_DUE_AFTER_DAYS: i64 = 2;
const DAILY_DUE_AFTER_DAYS: i64 = 1;
const HAPPY_CARE_STREAK_DAYS: i32 = 3;
const FOOD_DUE_PENALTY: i16 = 25;
const FOOD_OVERDUE_PENALTY: i16 = 55;
const WATER_DUE_PENALTY: i16 = 10;
const WATER_OVERDUE_PENALTY: i16 = 25;
/// Care score at or below which the pet reads `Sad` regardless of which need
/// is missing. Overdue food alone (45) clears this bar; so does any pair of
/// overdue needs.
const SAD_CARE_SCORE: u8 = 50;

const PET_ROAM_DURATION_SECS: i64 = 30 * 60;

impl PetNeeds {
    pub fn all_required_done(self) -> bool {
        self.food == PetNeedStatus::Done && self.water == PetNeedStatus::Done
    }

    pub fn care_score(self) -> u8 {
        let penalty = need_penalty(self.food, FOOD_DUE_PENALTY, FOOD_OVERDUE_PENALTY)
            + need_penalty(self.water, WATER_DUE_PENALTY, WATER_OVERDUE_PENALTY);
        (100 - penalty.clamp(0, 100)) as u8
    }
}

/// Outcome of a feed attempt, so the caller can send an out-of-food user to
/// the Shop while the strip carries the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOutcome {
    Fed,
    OutOfFood,
    AlreadyFedToday,
}

pub struct PetState {
    pub user_id: Uuid,
    pub svc: PetService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_watered: Option<DateTime<Utc>>,
    pub care_streak_days: i32,
    pub care_streak_date: Option<NaiveDate>,

    /// User-set pet name. `None` until set via the `/petname` chat command.
    pub name: Option<String>,

    /// Species of this pet (e.g. "cat", "dog"). Drives life-stage labels.
    pub species: String,

    /// When the cat row was first created. Used as a fallback age anchor.
    pub created: DateTime<Utc>,
    /// When the user unlocked the cat companion. Drives the life-stage buckets
    /// for purchased cats.
    pub adopted_at: Option<DateTime<Utc>>,

    pub action_feedback: Option<&'static str>,
    feedback_ticks: usize,
    animation_ticks: usize,
    roam_until: Option<DateTime<Utc>>,
    /// Mood and needs as of the previous tick, so the day-rollover flips
    /// (bowl colors, mood art) report as render-visible changes.
    last_visual: Option<(PetMood, PetNeeds)>,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl PetState {
    pub fn new(user_id: Uuid, svc: PetService, companion: PetCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            last_watered: companion.last_watered,
            care_streak_days: companion.care_streak_days,
            care_streak_date: companion.care_streak_date,
            name: companion.name,
            species: companion.species,
            created: companion.created,
            adopted_at: companion.adopted_at,
            action_feedback: None,
            feedback_ticks: 0,
            animation_ticks: 0,
            roam_until: None,
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
    /// frame even when the animation predicate is quiet: feedback expiry, a
    /// roam ending, and mood/needs flips at the UTC day rollover. Pure
    /// animation cadence is the strip's business (`ui::strip_frame_changed`).
    /// `wall_tick` is the app's shared 66ms wall clock (marquee_tick): the
    /// adaptive loop ticks sparsely, so a per-call counter would slow the
    /// animation with the cadence; syncing to the wall clock keeps every
    /// speed true at any wake tier.
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
        if self
            .roam_until
            .is_some_and(|roam_until| roam_until <= Utc::now())
        {
            self.roam_until = None;
            changed = true;
        }
        let visual = (self.mood(), self.needs());
        if self.last_visual != Some(visual) {
            self.last_visual = Some(visual);
            changed = true;
        }
        changed
    }

    pub fn mood(&self) -> PetMood {
        self.mood_at(Utc::now())
    }

    fn mood_at(&self, now: DateTime<Utc>) -> PetMood {
        mood_for_state(
            self.needs_on(now.date_naive()),
            self.care_streak_days,
            self.care_streak_date,
            now.date_naive(),
        )
    }

    pub fn needs(&self) -> PetNeeds {
        self.needs_on(Utc::now().date_naive())
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn roaming_active(&self) -> bool {
        self.roam_until
            .is_some_and(|roam_until| roam_until > Utc::now())
    }

    pub fn fed_today(&self) -> bool {
        fed_on(self.last_fed, Utc::now().date_naive())
    }

    /// A meal costs one pet food from the Shop inventory and sends the pet off
    /// on a full-screen stroll. Capped at one meal per UTC day, so a bowl that
    /// is already full cannot be spun into an endless roam.
    pub fn feed(&mut self, pet_food_quantity: i32) -> FeedOutcome {
        let now = Utc::now();
        if fed_on(self.last_fed, now.date_naive()) {
            self.set_feedback("already fed today");
            return FeedOutcome::AlreadyFedToday;
        }
        if pet_food_quantity <= 0 {
            self.set_feedback("buy pet food first");
            return FeedOutcome::OutOfFood;
        }

        self.last_fed = Some(now);
        self.roam_until = Some(now + Duration::seconds(PET_ROAM_DURATION_SECS));
        self.set_feedback("fed! strolling");
        self.svc.feed_task(self.user_id);
        self.record_care_completion_if_ready(now);
        FeedOutcome::Fed
    }

    pub fn water(&mut self) {
        let now = Utc::now();
        self.last_watered = Some(now);
        self.set_feedback("watered!");
        self.svc.water_task(self.user_id);
        self.record_care_completion_if_ready(now);
    }

    fn set_feedback(&mut self, feedback: &'static str) {
        self.action_feedback = Some(feedback);
        self.feedback_ticks = FEEDBACK_TICKS;
    }

    fn needs_on(&self, today: NaiveDate) -> PetNeeds {
        PetNeeds {
            food: need_after(self.last_fed, today, FOOD_DUE_AFTER_DAYS),
            water: need_after(self.last_watered, today, DAILY_DUE_AFTER_DAYS),
        }
    }

    fn record_care_completion_if_ready(&mut self, now: DateTime<Utc>) {
        let today = now.date_naive();
        if !self.needs_on(today).all_required_done() || self.care_streak_date == Some(today) {
            return;
        }

        self.care_streak_days =
            next_care_streak_days(self.care_streak_days, self.care_streak_date, today);
        self.care_streak_date = Some(today);
        self.svc.record_care_completed_task(self.user_id, today);
    }
}

/// Mood is a straight walk down the needs, worst first. With only two needs
/// left the care score already subsumes every "how many are overdue" test:
/// nothing below `SAD_CARE_SCORE` survives to the per-need checks, and
/// nothing above it has both needs met unless it is fully cared for.
fn mood_for_state(
    needs: PetNeeds,
    care_streak_days: i32,
    care_streak_date: Option<NaiveDate>,
    today: NaiveDate,
) -> PetMood {
    if needs.all_required_done()
        && care_streak_date == Some(today)
        && care_streak_days >= HAPPY_CARE_STREAK_DAYS
    {
        return PetMood::Happy;
    }

    if needs.care_score() < SAD_CARE_SCORE {
        return PetMood::Sad;
    }

    if needs.food.is_missing() {
        return PetMood::Hungry;
    }

    if needs.water.is_missing() {
        return PetMood::Thirsty;
    }

    PetMood::Content
}

fn need_penalty(status: PetNeedStatus, due: i16, overdue: i16) -> i16 {
    match status {
        PetNeedStatus::Done => 0,
        PetNeedStatus::Due => due,
        PetNeedStatus::Overdue => overdue,
    }
}

fn need_after(last: Option<DateTime<Utc>>, today: NaiveDate, due_after_days: i64) -> PetNeedStatus {
    match days_since(last, today) {
        Some(days) if days < due_after_days => PetNeedStatus::Done,
        Some(days) if days == due_after_days => PetNeedStatus::Due,
        Some(_) => PetNeedStatus::Overdue,
        None => PetNeedStatus::Due,
    }
}

fn days_since(last: Option<DateTime<Utc>>, today: NaiveDate) -> Option<i64> {
    last.map(|time| (today - time.date_naive()).num_days().max(0))
}

fn fed_on(last: Option<DateTime<Utc>>, today: NaiveDate) -> bool {
    last.is_some_and(|time| time.date_naive() == today)
}

fn next_care_streak_days(
    current_days: i32,
    current_date: Option<NaiveDate>,
    today: NaiveDate,
) -> i32 {
    match current_date.map(|date| (today - date).num_days()) {
        Some(0) => current_days.max(1),
        Some(1) => current_days.saturating_add(1).max(1),
        _ => 1,
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
