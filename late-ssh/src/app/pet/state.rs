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

    pub fn is_overdue(self) -> bool {
        self == PetNeedStatus::Overdue
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

const PET_ROAM_DURATION_SECS: i64 = 30 * 60;

impl PetNeeds {
    pub fn all_required_done(self) -> bool {
        self.food == PetNeedStatus::Done && self.water == PetNeedStatus::Done
    }

    pub fn missing_count(self) -> usize {
        [self.food, self.water]
            .into_iter()
            .filter(|status| status.is_missing())
            .count()
    }

    pub fn overdue_count(self) -> usize {
        [self.food, self.water]
            .into_iter()
            .filter(|status| status.is_overdue())
            .count()
    }

    pub fn care_score(self) -> u8 {
        let penalty = need_penalty(self.food, FOOD_DUE_PENALTY, FOOD_OVERDUE_PENALTY)
            + need_penalty(self.water, WATER_DUE_PENALTY, WATER_OVERDUE_PENALTY);
        (100 - penalty.clamp(0, 100)) as u8
    }
}

pub struct PetState {
    pub user_id: Uuid,
    pub svc: PetService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_watered: Option<DateTime<Utc>>,
    pub last_treated: Option<DateTime<Utc>>,
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
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl PetState {
    pub fn new(user_id: Uuid, svc: PetService, companion: PetCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            last_watered: companion.last_watered,
            last_treated: companion.last_treated,
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

    pub fn tick(&mut self) {
        self.animation_ticks = self.animation_ticks.wrapping_add(1);

        if self.action_feedback.is_some() {
            self.feedback_ticks = self.feedback_ticks.saturating_sub(1);
            if self.feedback_ticks == 0 {
                self.action_feedback = None;
            }
        }
        if self
            .roam_until
            .is_some_and(|roam_until| roam_until <= Utc::now())
        {
            self.roam_until = None;
        }
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

    pub fn treated_today(&self) -> bool {
        treated_on(self.last_treated, Utc::now().date_naive())
    }

    pub fn feed(&mut self) {
        let now = Utc::now();
        self.last_fed = Some(now);
        self.action_feedback = Some("fed!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.feed_task(self.user_id);
        self.record_care_completion_if_ready(now);
    }

    pub fn water(&mut self) {
        let now = Utc::now();
        self.last_watered = Some(now);
        self.action_feedback = Some("watered!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.water_task(self.user_id);
        self.record_care_completion_if_ready(now);
    }

    pub fn pet_with_food(&mut self, pet_food_quantity: i32) {
        if pet_food_quantity <= 0 {
            self.action_feedback = Some("buy pet food first");
            self.feedback_ticks = FEEDBACK_TICKS;
            return;
        }
        let now = Utc::now();
        if treated_on(self.last_treated, now.date_naive()) {
            self.action_feedback = Some("already had a treat today");
            self.feedback_ticks = FEEDBACK_TICKS;
            return;
        }

        self.last_treated = Some(now);
        self.roam_until = Some(now + Duration::seconds(PET_ROAM_DURATION_SECS));
        self.action_feedback = Some("treat! strolling");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.use_pet_food_task(self.user_id);
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

fn mood_for_state(
    needs: PetNeeds,
    care_streak_days: i32,
    care_streak_date: Option<NaiveDate>,
    today: NaiveDate,
) -> PetMood {
    let score = needs.care_score();

    if needs.all_required_done()
        && care_streak_date == Some(today)
        && care_streak_days >= HAPPY_CARE_STREAK_DAYS
    {
        return PetMood::Happy;
    }

    if score < 50
        || needs.overdue_count() >= 2
        || (needs.food.is_overdue() && needs.missing_count() >= 2)
    {
        return PetMood::Sad;
    }

    if needs.food.is_missing() {
        return PetMood::Hungry;
    }

    if needs.water.is_overdue() {
        return PetMood::Thirsty;
    }

    if score >= 70 {
        return PetMood::Content;
    }

    if needs.water.is_missing() {
        return PetMood::Thirsty;
    }

    PetMood::Sad
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

fn treated_on(last: Option<DateTime<Utc>>, today: NaiveDate) -> bool {
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
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn food_is_due_every_two_days_while_water_is_daily() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let yesterday = Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
        let two_days = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
        let three_days = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();

        assert_eq!(
            need_after(Some(yesterday), today, FOOD_DUE_AFTER_DAYS),
            PetNeedStatus::Done
        );
        assert_eq!(
            need_after(Some(two_days), today, FOOD_DUE_AFTER_DAYS),
            PetNeedStatus::Due
        );
        assert_eq!(
            need_after(Some(three_days), today, FOOD_DUE_AFTER_DAYS),
            PetNeedStatus::Overdue
        );
        assert_eq!(
            need_after(Some(yesterday), today, DAILY_DUE_AFTER_DAYS),
            PetNeedStatus::Due
        );
        assert_eq!(
            need_after(Some(two_days), today, DAILY_DUE_AFTER_DAYS),
            PetNeedStatus::Overdue
        );
    }

    #[test]
    fn weighted_needs_drive_mood() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let cared = PetNeeds {
            food: PetNeedStatus::Done,
            water: PetNeedStatus::Done,
        };
        assert_eq!(
            mood_for_state(cared, HAPPY_CARE_STREAK_DAYS, Some(today), today),
            PetMood::Happy
        );
        assert_eq!(
            mood_for_state(cared, HAPPY_CARE_STREAK_DAYS - 1, Some(today), today),
            PetMood::Content
        );
        assert_eq!(
            mood_for_state(
                cared,
                HAPPY_CARE_STREAK_DAYS,
                Some(today.pred_opt().unwrap()),
                today
            ),
            PetMood::Content
        );

        assert_eq!(
            mood_for_state(
                PetNeeds {
                    water: PetNeedStatus::Due,
                    ..cared
                },
                HAPPY_CARE_STREAK_DAYS,
                Some(today),
                today,
            ),
            PetMood::Content
        );
        assert_eq!(
            mood_for_state(
                PetNeeds {
                    water: PetNeedStatus::Overdue,
                    ..cared
                },
                HAPPY_CARE_STREAK_DAYS,
                Some(today),
                today,
            ),
            PetMood::Thirsty
        );
        assert_eq!(
            mood_for_state(
                PetNeeds {
                    food: PetNeedStatus::Due,
                    ..cared
                },
                HAPPY_CARE_STREAK_DAYS,
                Some(today),
                today,
            ),
            PetMood::Hungry
        );
        assert_eq!(
            mood_for_state(
                PetNeeds {
                    food: PetNeedStatus::Overdue,
                    water: PetNeedStatus::Due,
                },
                HAPPY_CARE_STREAK_DAYS,
                Some(today),
                today,
            ),
            PetMood::Sad
        );
        assert_eq!(
            mood_for_state(
                PetNeeds {
                    food: PetNeedStatus::Overdue,
                    water: PetNeedStatus::Overdue,
                },
                HAPPY_CARE_STREAK_DAYS,
                Some(today),
                today,
            ),
            PetMood::Sad
        );
    }

    #[test]
    fn completed_care_streak_advances_by_calendar_day() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let two_days_ago = yesterday.pred_opt().unwrap();

        assert_eq!(next_care_streak_days(0, None, today), 1);
        assert_eq!(next_care_streak_days(1, Some(today), today), 1);
        assert_eq!(next_care_streak_days(2, Some(yesterday), today), 3);
        assert_eq!(next_care_streak_days(8, Some(two_days_ago), today), 1);
    }

    #[test]
    fn care_score_weights_food_more_than_water() {
        let cared = PetNeeds {
            food: PetNeedStatus::Done,
            water: PetNeedStatus::Done,
        };
        assert_eq!(
            PetNeeds {
                water: PetNeedStatus::Due,
                ..cared
            }
            .care_score(),
            90
        );
        assert_eq!(
            PetNeeds {
                food: PetNeedStatus::Due,
                ..cared
            }
            .care_score(),
            75
        );
        assert_eq!(
            PetNeeds {
                food: PetNeedStatus::Overdue,
                ..cared
            }
            .care_score(),
            45
        );
    }
}
