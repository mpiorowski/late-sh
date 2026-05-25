use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::cat::CatCompanion;
use uuid::Uuid;

use super::svc::CatService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatMood {
    Happy,
    Content,
    Bored,
    Hungry,
    Thirsty,
    Sad,
}

impl CatMood {
    pub fn label(self) -> &'static str {
        match self {
            CatMood::Happy => "happy",
            CatMood::Content => "content",
            CatMood::Bored => "bored",
            CatMood::Hungry => "hungry",
            CatMood::Thirsty => "thirsty",
            CatMood::Sad => "sad",
        }
    }

    pub fn eyes(self) -> &'static str {
        match self {
            CatMood::Happy => "^.^",
            CatMood::Content => "o.o",
            CatMood::Bored => "-.-",
            CatMood::Hungry => "o.o",
            CatMood::Thirsty => "o_o",
            CatMood::Sad => "T_T",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatNeedStatus {
    Done,
    Due,
    Overdue,
}

impl CatNeedStatus {
    pub fn label(self) -> &'static str {
        match self {
            CatNeedStatus::Done => "ok",
            CatNeedStatus::Due => "due",
            CatNeedStatus::Overdue => "late",
        }
    }

    pub fn is_missing(self) -> bool {
        self != CatNeedStatus::Done
    }

    pub fn is_overdue(self) -> bool {
        self == CatNeedStatus::Overdue
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatNeeds {
    pub food: CatNeedStatus,
    pub water: CatNeedStatus,
    pub play: CatNeedStatus,
}

pub const PLAY_RUN_NEEDED: u16 = 100;

const PLAY_FIELD_MAX: i16 = 1000;
const PLAY_TOY_STEP: i16 = 75;
const PLAY_TOY_DASH: i16 = 180;
const PLAY_CATCH_RADIUS: i16 = 95;
const PLAY_POUNCE_PENALTY: u16 = 18;
const PLAY_MESSAGE_TICKS: usize = 15 * 2;
const PLAY_POUNCE_COOLDOWN_TICKS: usize = 15;

impl CatNeeds {
    pub fn all_required_done(self) -> bool {
        self.food == CatNeedStatus::Done
            && self.water == CatNeedStatus::Done
            && self.play == CatNeedStatus::Done
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
pub struct CatPlayState {
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

impl CatPlayState {
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

    fn tick(&mut self, mood: CatMood) -> bool {
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

pub struct CatState {
    pub user_id: Uuid,
    pub svc: CatService,

    pub last_fed: Option<DateTime<Utc>>,
    pub last_watered: Option<DateTime<Utc>>,
    pub last_played: Option<DateTime<Utc>>,

    /// User-set pet name. `None` until set via the `/petname` chat command.
    pub name: Option<String>,

    pub action_feedback: Option<&'static str>,
    feedback_ticks: usize,
    animation_ticks: usize,
    play: Option<CatPlayState>,
}

const FEEDBACK_TICKS: usize = 15 * 2;

impl CatState {
    pub fn new(user_id: Uuid, svc: CatService, companion: CatCompanion) -> Self {
        Self {
            user_id,
            svc,
            last_fed: companion.last_fed,
            last_watered: companion.last_watered,
            last_played: companion.last_played,
            name: companion.name,
            action_feedback: None,
            feedback_ticks: 0,
            animation_ticks: 0,
            play: None,
        }
    }

    /// Set (or clear with `None`) the user-set pet name and persist it.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name.clone();
        self.svc.set_name_task(self.user_id, name);
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

    pub fn mood(&self) -> CatMood {
        mood_for_needs(self.needs())
    }

    pub fn needs(&self) -> CatNeeds {
        self.needs_on(Utc::now().date_naive())
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn play_session(&self) -> Option<&CatPlayState> {
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
            self.play = Some(CatPlayState::new());
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

    fn needs_on(&self, today: NaiveDate) -> CatNeeds {
        CatNeeds {
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

fn chase_speed(mood: CatMood) -> i16 {
    match mood {
        CatMood::Happy => 23,
        CatMood::Content => 20,
        CatMood::Bored => 18,
        CatMood::Hungry | CatMood::Thirsty => 14,
        CatMood::Sad => 10,
    }
}

fn mood_for_needs(needs: CatNeeds) -> CatMood {
    let overdue_count = needs.overdue_count();
    if overdue_count >= 2 || (overdue_count == 1 && needs.missing_count() >= 3) {
        return CatMood::Sad;
    }
    if needs.water.is_missing() {
        return CatMood::Thirsty;
    }
    if needs.food.is_missing() {
        return CatMood::Hungry;
    }
    if needs.play.is_missing() {
        return CatMood::Bored;
    }
    CatMood::Happy
}

fn daily_need(last: Option<DateTime<Utc>>, today: NaiveDate) -> CatNeedStatus {
    match days_since(last, today) {
        Some(0) => CatNeedStatus::Done,
        Some(1) | None => CatNeedStatus::Due,
        Some(_) => CatNeedStatus::Overdue,
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

        assert_eq!(daily_need(Some(yesterday), today), CatNeedStatus::Due);
        assert_eq!(daily_need(Some(two_days), today), CatNeedStatus::Overdue);
    }

    #[test]
    fn combined_needs_drive_mood() {
        let cared = CatNeeds {
            food: CatNeedStatus::Done,
            water: CatNeedStatus::Done,
            play: CatNeedStatus::Done,
        };
        assert_eq!(mood_for_needs(cared), CatMood::Happy);

        assert_eq!(
            mood_for_needs(CatNeeds {
                play: CatNeedStatus::Due,
                ..cared
            }),
            CatMood::Bored
        );
        assert_eq!(
            mood_for_needs(CatNeeds {
                food: CatNeedStatus::Overdue,
                water: CatNeedStatus::Overdue,
                ..cared
            }),
            CatMood::Sad
        );
        assert_eq!(
            mood_for_needs(CatNeeds {
                water: CatNeedStatus::Due,
                ..cared
            }),
            CatMood::Thirsty
        );
        assert_eq!(
            mood_for_needs(CatNeeds {
                food: CatNeedStatus::Overdue,
                water: CatNeedStatus::Due,
                play: CatNeedStatus::Due,
            }),
            CatMood::Sad
        );
    }

    #[test]
    fn play_session_gains_energy_when_cat_runs() {
        let mut play = CatPlayState::new();
        play.toy_x = PLAY_FIELD_MAX;
        play.toy_y = 0;
        play.cat_x = 0;
        play.cat_y = PLAY_FIELD_MAX;

        for _ in 0..10 {
            play.tick(CatMood::Happy);
        }

        assert!(play.run_energy > 0);
        assert!(play.run_energy < PLAY_RUN_NEEDED);
    }

    #[test]
    fn play_session_pounce_penalizes_progress() {
        let mut play = CatPlayState::new();
        play.run_energy = 50;
        play.toy_x = play.cat_x;
        play.toy_y = play.cat_y;

        play.tick(CatMood::Happy);

        assert_eq!(play.pounces, 1);
        assert_eq!(play.run_energy, 50 - PLAY_POUNCE_PENALTY);
    }
}
