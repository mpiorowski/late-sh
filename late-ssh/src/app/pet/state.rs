use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::pet::{PetCompanion, LifeStage, pet_age_anchor, pet_age_label};
use uuid::Uuid;

use super::svc::PetService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    Happy,
    Content,
    Bored,
    Hungry,
    Thirsty,
    Sad,
}

impl PetMood {
    pub fn label(self) -> &'static str {
        match self {
            PetMood::Happy => "happy",
            PetMood::Content => "content",
            PetMood::Bored => "bored",
            PetMood::Hungry => "hungry",
            PetMood::Thirsty => "thirsty",
            PetMood::Sad => "sad",
        }
    }

    pub fn eyes(self) -> &'static str {
        match self {
            PetMood::Happy => "^.^",
            PetMood::Content => "o.o",
            PetMood::Bored => "-.-",
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
    pub play: PetNeedStatus,
}

pub const PLAY_RUN_NEEDED: u16 = 100;

const PLAY_FIELD_MAX: i16 = 1000;
const PLAY_TOY_STEP: i16 = 75;
const PLAY_TOY_DASH: i16 = 180;
const PLAY_CATCH_RADIUS: i16 = 95;
const PLAY_POUNCE_PENALTY: u16 = 18;
const PLAY_MESSAGE_TICKS: usize = 15 * 2;
const PLAY_POUNCE_COOLDOWN_TICKS: usize = 15;

impl PetNeeds {
    pub fn all_required_done(self) -> bool {
        self.food == PetNeedStatus::Done
            && self.water == PetNeedStatus::Done
            && self.play == PetNeedStatus::Done
    }

    pub fn missing_count(self) -> usize {
        [self.food, self.water, self.play]
            .into_iter()
            .filter(|status| status.is_missing())
            .count()
    }

    pub fn overdue_count(self) -> usize {
        [self.food, self.water, self.play]
            .into_iter()
            .filter(|status| status.is_overdue())
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetPlayState {
    pub toy_x: i16,
    pub toy_y: i16,
    pub cat_x: i16,
    pub cat_y: i16,
    pub run_energy: u16,
    pub pounces: u8,
    pub message: &'static str,
    message_ticks: usize,
    pounce_cooldown: usize,
}

impl PetPlayState {
    fn new() -> Self {
        Self {
            toy_x: PLAY_FIELD_MAX / 2,
            toy_y: PLAY_FIELD_MAX / 4,
            cat_x: PLAY_FIELD_MAX / 2,
            cat_y: PLAY_FIELD_MAX * 3 / 4,
            run_energy: 0,
            pounces: 0,
            message: "keep the toy away",
            message_ticks: PLAY_MESSAGE_TICKS,
            pounce_cooldown: 0,
        }
    }

    fn tick(&mut self, mood: PetMood) -> bool {
        self.message_ticks = self.message_ticks.saturating_sub(1);
        self.pounce_cooldown = self.pounce_cooldown.saturating_sub(1);
        if self.message_ticks == 0 {
            self.message = "run!";
        }

        let old_cat_x = self.cat_x;
        let old_cat_y = self.cat_y;
        let speed = chase_speed(mood);
        self.cat_x = step_toward(self.cat_x, self.toy_x, speed);
        self.cat_y = step_toward(self.cat_y, self.toy_y, speed);

        let distance = self.distance_to_toy();
        if distance <= PLAY_CATCH_RADIUS && self.pounce_cooldown == 0 {
            self.pounces = self.pounces.saturating_add(1);
            self.run_energy = self.run_energy.saturating_sub(PLAY_POUNCE_PENALTY);
            self.pounce_cooldown = PLAY_POUNCE_COOLDOWN_TICKS;
            self.set_message("pounced!");
            return false;
        }

        let moved = (self.cat_x - old_cat_x).abs() + (self.cat_y - old_cat_y).abs();
        if moved > 0 && distance > PLAY_CATCH_RADIUS {
            let gain = if distance > 420 { 2 } else { 1 };
            self.run_energy = (self.run_energy + gain).min(PLAY_RUN_NEEDED);
        }

        if self.run_energy >= PLAY_RUN_NEEDED {
            self.set_message("zoomies!");
            true
        } else {
            false
        }
    }

    fn move_toy(&mut self, dx: i16, dy: i16) {
        self.toy_x = (self.toy_x + dx).clamp(0, PLAY_FIELD_MAX);
        self.toy_y = (self.toy_y + dy).clamp(0, PLAY_FIELD_MAX);
        if self.message != "pounced!" {
            self.message = "run!";
        }
    }

    fn dash_toy(&mut self) {
        let dx = (self.toy_x - self.cat_x).signum();
        let dy = (self.toy_y - self.cat_y).signum();
        let dx = if dx == 0 { 1 } else { dx };
        let dy = if dy == 0 { 1 } else { dy };
        self.move_toy(dx * PLAY_TOY_DASH, dy * PLAY_TOY_DASH);
        self.set_message("dash!");
    }

    fn set_message(&mut self, message: &'static str) {
        self.message = message;
        self.message_ticks = PLAY_MESSAGE_TICKS;
    }

    fn distance_to_toy(&self) -> i16 {
        (self.cat_x - self.toy_x)
            .abs()
            .max((self.cat_y - self.toy_y).abs())
    }
}

pub struct PetState {
    pub user_id: Uuid,
    pub svc: PetService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_watered: Option<DateTime<Utc>>,
    pub last_played: Option<DateTime<Utc>>,

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
    play: Option<PetPlayState>,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl PetState {
    pub fn new(user_id: Uuid, svc: PetService, companion: PetCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            last_watered: companion.last_watered,
            last_played: companion.last_played,
            name: companion.name,
            species: companion.species,
            created: companion.created,
            adopted_at: companion.adopted_at,
            action_feedback: None,
            feedback_ticks: 0,
            animation_ticks: 0,
            play: None,
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
        let mood = self.mood();
        let play_complete = self.play.as_mut().is_some_and(|play| play.tick(mood));
        if play_complete {
            self.complete_play();
        }

        if self.action_feedback.is_some() {
            self.feedback_ticks = self.feedback_ticks.saturating_sub(1);
            if self.feedback_ticks == 0 {
                self.action_feedback = None;
            }
        }
    }

    pub fn mood(&self) -> PetMood {
        mood_for_needs(self.needs())
    }

    pub fn needs(&self) -> PetNeeds {
        self.needs_on(Utc::now().date_naive())
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn play_session(&self) -> Option<&PetPlayState> {
        self.play.as_ref()
    }

    pub fn feed(&mut self) {
        self.play = None;
        self.last_fed = Some(Utc::now());
        self.action_feedback = Some("fed!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.feed_task(self.user_id);
    }

    pub fn water(&mut self) {
        self.play = None;
        self.last_watered = Some(Utc::now());
        self.action_feedback = Some("watered!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.water_task(self.user_id);
    }

    pub fn play(&mut self) {
        if self.play.is_none() {
            self.action_feedback = None;
            self.play = Some(PetPlayState::new());
        } else {
            self.dash_play_toy();
        }
    }

    pub fn move_play_toy_left(&mut self) {
        if let Some(play) = &mut self.play {
            play.move_toy(-PLAY_TOY_STEP, 0);
        }
    }

    pub fn move_play_toy_right(&mut self) {
        if let Some(play) = &mut self.play {
            play.move_toy(PLAY_TOY_STEP, 0);
        }
    }

    pub fn move_play_toy_up(&mut self) {
        if let Some(play) = &mut self.play {
            play.move_toy(0, -PLAY_TOY_STEP);
        }
    }

    pub fn move_play_toy_down(&mut self) {
        if let Some(play) = &mut self.play {
            play.move_toy(0, PLAY_TOY_STEP);
        }
    }

    pub fn dash_play_toy(&mut self) {
        if let Some(play) = &mut self.play {
            play.dash_toy();
        }
    }

    pub fn cancel_play(&mut self) {
        if self.play.take().is_some() {
            self.action_feedback = Some("play stopped");
            self.feedback_ticks = FEEDBACK_TICKS;
        }
    }

    fn needs_on(&self, today: NaiveDate) -> PetNeeds {
        PetNeeds {
            food: daily_need(self.last_fed, today),
            water: daily_need(self.last_watered, today),
            play: daily_need(self.last_played, today),
        }
    }

    fn complete_play(&mut self) {
        self.play = None;
        self.last_played = Some(Utc::now());
        self.action_feedback = Some("played!");
        self.feedback_ticks = FEEDBACK_TICKS;
        self.svc.play_task(self.user_id);
    }
}

fn step_toward(current: i16, target: i16, step: i16) -> i16 {
    let delta = target - current;
    if delta.abs() <= step {
        target
    } else {
        current + step * delta.signum()
    }
}

fn chase_speed(mood: PetMood) -> i16 {
    match mood {
        PetMood::Happy => 23,
        PetMood::Content => 20,
        PetMood::Bored => 18,
        PetMood::Hungry | PetMood::Thirsty => 14,
        PetMood::Sad => 10,
    }
}

fn mood_for_needs(needs: PetNeeds) -> PetMood {
    let overdue_count = needs.overdue_count();
    if overdue_count >= 2 || (overdue_count == 1 && needs.missing_count() >= 3) {
        return PetMood::Sad;
    }
    if needs.water.is_missing() {
        return PetMood::Thirsty;
    }
    if needs.food.is_missing() {
        return PetMood::Hungry;
    }
    if needs.play.is_missing() {
        return PetMood::Bored;
    }
    PetMood::Happy
}

fn daily_need(last: Option<DateTime<Utc>>, today: NaiveDate) -> PetNeedStatus {
    match days_since(last, today) {
        Some(0) => PetNeedStatus::Done,
        Some(1) | None => PetNeedStatus::Due,
        Some(_) => PetNeedStatus::Overdue,
    }
}

fn days_since(last: Option<DateTime<Utc>>, today: NaiveDate) -> Option<i64> {
    last.map(|time| (today - time.date_naive()).num_days().max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn daily_needs_are_due_tomorrow_and_overdue_after_that() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let yesterday = Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
        let two_days = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();

        assert_eq!(daily_need(Some(yesterday), today), PetNeedStatus::Due);
        assert_eq!(daily_need(Some(two_days), today), PetNeedStatus::Overdue);
    }

    #[test]
    fn combined_needs_drive_mood() {
        let cared = PetNeeds {
            food: PetNeedStatus::Done,
            water: PetNeedStatus::Done,
            play: PetNeedStatus::Done,
        };
        assert_eq!(mood_for_needs(cared), PetMood::Happy);

        assert_eq!(
            mood_for_needs(PetNeeds {
                play: PetNeedStatus::Due,
                ..cared
            }),
            PetMood::Bored
        );
        assert_eq!(
            mood_for_needs(PetNeeds {
                food: PetNeedStatus::Overdue,
                water: PetNeedStatus::Overdue,
                ..cared
            }),
            PetMood::Sad
        );
        assert_eq!(
            mood_for_needs(PetNeeds {
                water: PetNeedStatus::Due,
                ..cared
            }),
            PetMood::Thirsty
        );
        assert_eq!(
            mood_for_needs(PetNeeds {
                food: PetNeedStatus::Overdue,
                water: PetNeedStatus::Due,
                play: PetNeedStatus::Due,
            }),
            PetMood::Sad
        );
    }

    #[test]
    fn play_session_gains_energy_when_cat_runs() {
        let mut play = PetPlayState::new();
        play.toy_x = PLAY_FIELD_MAX;
        play.toy_y = 0;
        play.cat_x = 0;
        play.cat_y = PLAY_FIELD_MAX;

        for _ in 0..10 {
            play.tick(PetMood::Happy);
        }

        assert!(play.run_energy > 0);
        assert!(play.run_energy < PLAY_RUN_NEEDED);
    }

    #[test]
    fn play_session_pounce_penalizes_progress() {
        let mut play = PetPlayState::new();
        play.run_energy = 50;
        play.toy_x = play.cat_x;
        play.toy_y = play.cat_y;

        play.tick(PetMood::Happy);

        assert_eq!(play.pounces, 1);
        assert_eq!(play.run_energy, 50 - PLAY_POUNCE_PENALTY);
    }
}
